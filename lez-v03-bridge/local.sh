#!/usr/bin/env bash
# EXPERIMENTAL (README.md). The whole round to the burns on a LOCAL LEZ v0.3.1 standalone sequencer (its debug
# configuration, Bedrock mocked, proofs verified) and a plain local anvil, through testnet.sh run, then verify.
#
#   lez-v03-bridge/local.sh RUN_DIR      (RUN_DIR outside the repository; it must not exist yet)
#
# Needs: LEZ v0.3.1 at ../../upstream/lez-v0.3.1 with `cargo build --release --features standalone -p
# sequencer_service` done there; `cargo build --release -p driver03` here; Foundry (anvil, forge, cast) and
# vault/lib installed (vault/README.md); jq, curl, python3, /usr/bin/time. The payer is a fresh key the run
# makes; the sequencer's genesis funds it (a copy of LEZ's debug configuration with one more supply account).
# Without RISC0_DEV_MODE the wallet proves the private transactions for real and the sequencer verifies them.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
lez_src="$(cd "$repo/../upstream/lez-v0.3.1" && pwd)"
run="${1:?RUN_DIR}"
case "$run" in /*) ;; *) run="$PWD/$run" ;; esac
case "$run" in "$repo"/*) echo "local.sh: RUN_DIR must be outside the repository" >&2; exit 1 ;; esac
[ ! -e "$run" ] || { echo "local.sh: $run exists" >&2; exit 1; }
mkdir -p "$run/seq"
drv="$here/target/release/bridge03"
seq_bin="$lez_src/target/release/sequencer_service"
for f in "$drv" "$seq_bin"; do [ -x "$f" ] || { echo "local.sh: build $f first" >&2; exit 1; }; done
log() { printf '== %s %s\n' "$(date -u +%T)" "$*" | tee -a "$run/local.log"; }
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true; }
trap cleanup EXIT

head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "$run/payer.key"
"$drv" "$run" setup --sequencer http://127.0.0.1:3040 --payer "$run/payer.key" > "$run/setup.log"
payer="$(jq -r .payer "$run/run.json")"
jq --arg p "$payer" '.genesis += [{"supply_account": {"account_id": $p, "balance": 1000000000000000}}]' \
  "$lez_src/lez/sequencer/service/configs/debug/sequencer_config.json" > "$run/seq/sequencer_config.json"
log "payer $payer funded at genesis"

log "anvil"
anvil --port 8545 --silent > "$run/anvil.log" 2>&1 & pids+=($!)
log "sequencer (standalone, LEZ v0.3.1 $(git -C "$lez_src" rev-parse --short HEAD))"
( cd "$run/seq" && RUST_LOG=info exec "$seq_bin" sequencer_config.json --port 3040 --listen-address 127.0.0.1 ) > "$run/seq/sequencer.log" 2>&1 & pids+=($!)
for _ in $(seq 1 120); do
  curl -s --max-time 2 -H 'Content-Type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' http://127.0.0.1:3040 | grep -q result && break
  sleep 2
done
curl -s --max-time 5 -H 'Content-Type: application/json' --data '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId","params":[]}' http://127.0.0.1:3040 | tee -a "$run/local.log"; echo
cast chain-id --rpc-url http://127.0.0.1:8545 | tee -a "$run/local.log"

log "testnet.sh run (FERRATA_NET=local)"
FERRATA_NET=local FERRATA_RUN="$run" FERRATA_TESTNET_SEND=yes FERRATA_PAYER_KEY="$run/payer.key" BRIDGE03="$drv" \
  "$here/testnet.sh" run 2>&1 | tee -a "$run/local.log"
log "done"
