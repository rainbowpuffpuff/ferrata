#!/usr/bin/env bash
# Ferrata's light client on LEZ v0.3.1, the public LEZ testnet's version (EXPERIMENTAL, UNAUDITED: README.md).
#
#   ./testnet.sh plan [--fresh]   print every transaction `run` sends, its size, its execution gas against the 10M cap,
#                                 the funds the payer needs and how the LEZ docs say to get them; make the payer's
#                                 fresh key (outside the repository) if there is none. Sends nothing. Simulates on LEZ
#                                 v0.3.1's state machine in this process, on the committed local run's data, or, with
#                                 --fresh, on a new checkpoint and the next finality update (waits up to ~13 min).
#   ./testnet.sh run              deploy the program, Init from the newest Sepolia finalized checkpoint, two finality
#                                 updates (one transaction each), one attestation of the Sepolia vault's deposit
#                                 accumulator (slots 0 and 1); resumable and idempotent (every step reads the chain
#                                 first). Refuses to start unless FERRATA_TESTNET_SEND=yes.
#   ./testnet.sh verify           re-read every verdict from the sequencer's RPC (and, for the public testnet, the
#                                 explorer) and compare with the committed record; exit 0 only if all match.
#   ./testnet.sh local            the same run against a LEZ v0.3.1 sequencer started here (its standalone mode,
#                                 mock Bedrock), then verify; records under results/local/.
#
# Target: the public testnet (https://testnet.lez.logos.co) unless FERRATA_SEQUENCER is set. Keys and run data live
# under $FERRATA_STATE (default spike/tmp/testnet-v03/<public|local>), never in the repository; the record of a run
# (no keys) is copied to results/<public|local>/. Build first: scripts/build.sh --lez.
set -euo pipefail
cd "$(dirname "$0")"
HERE="$PWD"
SPIKE="$(cd ../.. && pwd)"
PUBLIC_URL="https://testnet.lez.logos.co"
EXPLORER="https://explorer.testnet.lez.logos.co"
VAULT="0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c"   # FerrataVault v5 on Sepolia (docs/sepolia.md)
VAULT_SLOTS="0,1"                                      # depositRoot, nextLeafIndex (as the v0.4 bridge's ImportRoot)
CLIENT="$HERE/target/release/ferrata-v03"
LEZ_BIN="$HERE/target/lez/release"
UA="ferrata-v03-check"

cmd="${1:-}"; shift || true
target="public"
if [ "$cmd" = local ] || [ "${FERRATA_SEQUENCER:-$PUBLIC_URL}" != "$PUBLIC_URL" ]; then target="local"; fi
URL="${FERRATA_SEQUENCER:-$PUBLIC_URL}"
[ "$cmd" = local ] && URL="${FERRATA_SEQUENCER:-http://127.0.0.1:3040}"
STATE="${FERRATA_STATE:-$SPIKE/tmp/testnet-v03/$target}"
RUN="$STATE/run"
RECORD="$HERE/results/$target"
export LEE_WALLET_HOME_DIR="$STATE/wallet"
mkdir -p "$STATE"

need_client() { [ -x "$CLIENT" ] || { echo "build first: scripts/build.sh --lez" >&2; exit 2; }; }

rpc() { # rpc METHOD PARAMS_JSON: one read-only JSON-RPC call to the sequencer
  curl -sS -m 30 -A "$UA" -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":$2}" "$URL"
}

ensure_wallet() {
  if [ ! -f "$LEE_WALLET_HOME_DIR/storage.json" ]; then
    mkdir -p "$RUN"
    "$CLIENT" new-wallet "$RUN" "$URL" > "$STATE/payer"
    echo "a fresh key was made under $LEE_WALLET_HOME_DIR (never in the repository)"
  fi
  [ -f "$STATE/payer" ] || python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["payer"])' "$RUN/state.json" > "$STATE/payer"
  PAYER="$(cat "$STATE/payer")"
}

balance() { rpc getAccountBalance "[\"$PAYER\"]" | python3 -I -c 'import json,sys; print(json.load(sys.stdin).get("result", 0))'; }

copy_record() { # the run's record, without keys, into the repository's results
  mkdir -p "$RECORD"
  for f in chain.json state.json applied.txt txs.tsv record.json requests.tsv plan.json; do
    [ -f "$RUN/$f" ] && cp "$RUN/$f" "$RECORD/$f"
  done
  rm -rf "$RECORD/updates" "$RECORD/proofs"
  [ -d "$RUN/updates" ] && cp -r "$RUN/updates" "$RECORD/updates"
  [ -d "$RUN/proofs" ] && cp -r "$RUN/proofs" "$RECORD/proofs"
  cp "$RUN/bootstrap.ssz" "$RECORD/bootstrap.ssz" 2>/dev/null || true
  echo "record: $RECORD"
}

plan() {
  need_client
  ensure_wallet
  local data="$HERE/results/local"
  if [ "${1:-}" = --fresh ]; then
    data="$STATE/plan-data"; rm -rf "$data"; mkdir -p "$data"
    python3 -I scripts/fetch.py checkpoint "$data"
    local slot; slot="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["bootstrap_slot"])' "$data/chain.json")"
    local f; f="$(python3 -I scripts/fetch.py finality "$data" "$slot" 900)"
    local fin; fin="$(basename "$f" .ssz | sed 's/.*-finality-//')"
    local hash; hash="$(python3 -I - "$f" <<'EOF'
import sys
b = open(sys.argv[1], "rb").read()
H = 112 + 32 + 11 * 32
print("0x" + b[H + 112:H + 144].hex())
EOF
)"
    python3 -I scripts/fetch.py proof "$data" "$fin" "$hash" "$VAULT" "$VAULT_SLOTS"
  fi
  [ -f "$data/chain.json" ] || { echo "no data at $data: run ./testnet.sh local first, or plan --fresh" >&2; exit 2; }
  echo "== plan: the transactions of a run, simulated on LEZ v0.3.1's state machine (data: $data)"
  "$CLIENT" plan "$data" | tee "$STATE/plan.txt"
  echo
  echo "== the target"
  echo "sequencer: $URL"
  echo "fee state now: $(rpc getFeeState '[]')"
  echo "payer (fresh key, under $LEE_WALLET_HOME_DIR): $PAYER"
  echo "payer balance now: $(balance)"
  cat <<EOF

== funds
The payer pays every fee. Each program transaction declares the 10,000,000 gas cap: the sequencer reserves
gas_limit x base_fee_exec + bytes x base_fee_stor before running it and refunds the unused part in the same block,
so the payer must hold that reserve (about 80.5M native units at base fee 8; 12.5% more for each consecutive block
above 5M gas) on top of the fees above. Fund the payer with at least 200,000,000 native units (the fees of the plan
plus the largest reserve at a doubled base fee, with margin).
How the LEZ docs say to get testnet funds (logos-docs: docs/lez/get-started/run-lez-wallet-via-cli.md, "Fund the
sender account"; the endpoints page lists v0.3.x): bridge tokens from the Logos Blockchain with a channel deposit
(docs/blockchain/transfer-tokens/bridge-assets-from-logos-blockchain-to-zone-on-basecamp.md; in Metadata, paste the
payer's account id without its Public/ prefix), or have another testnet user transfer native tokens to it.
This script never asks a faucet and never funds anything: the owner does.
EOF
}

run() {
  need_client
  if [ "${FERRATA_TESTNET_SEND:-}" != yes ]; then
    echo "refusing to send: set FERRATA_TESTNET_SEND=yes to run against $URL" >&2; exit 1
  fi
  ensure_wallet
  mkdir -p "$RUN"
  echo "== run against $URL, payer $PAYER (balance $(balance))"
  [ -f "$RUN/chain.json" ] || python3 -I scripts/fetch.py checkpoint "$RUN"
  "$CLIENT" deploy "$RUN"
  "$CLIENT" init "$RUN"
  local want=2 have f out rc p pf after
  while :; do
    # Apply every fetched update the instance does not hold yet, in order; an update signed in the store's next
    # period needs the store's period update first (fetched and applied, then the update again).
    for f in $(ls "$RUN/updates" 2>/dev/null | sort); do
      while :; do
        set +e; out="$("$CLIENT" update "$RUN" "$RUN/updates/$f" 2>&1)"; rc=$?; set -e
        printf '%s\n' "$out"
        if [ $rc -eq 3 ]; then
          p="$(printf '%s' "$out" | sed -n 's/.*NEEDS-PERIOD-UPDATE \([0-9]*\).*/\1/p' | head -1)"
          pf="$(python3 -I scripts/fetch.py period "$RUN" "$p")"
          "$CLIENT" update "$RUN" "$pf"
          continue
        fi
        [ $rc -eq 0 ] || exit $rc
        break
      done
    done
    have="$(ls "$RUN/updates" 2>/dev/null | grep -c -- '-finality-' || true)"
    [ "$have" -ge "$want" ] && break
    after="$("$CLIENT" finalized "$RUN" | cut -d' ' -f1)"
    python3 -I scripts/fetch.py finality "$RUN" "$after" 1200 >/dev/null
  done
  # The attestation: the vault's slots at the execution block the latest record binds.
  read -r slot hash < <("$CLIENT" finalized "$RUN")
  [ -f "$RUN/proofs/$slot/proof.json" ] || python3 -I scripts/fetch.py proof "$RUN" "$slot" "$hash" "$VAULT" "$VAULT_SLOTS"
  "$CLIENT" attest "$RUN" "$RUN/proofs/$slot/proof.json" "$RUN/proofs/$slot/header.rlp"
  "$CLIENT" record "$RUN"
  copy_record
  echo "== done: $(grep -c . "$RUN/txs.tsv") lines in txs.tsv; payer balance $(balance)"
}

verify() {
  need_client
  [ -f "$RECORD/record.json" ] || { echo "no record at $RECORD" >&2; exit 2; }
  local tmp; tmp="$(mktemp -d "$STATE/verify.XXXXXX")"
  cp -r "$RECORD/." "$tmp/"
  local rc=0
  "$CLIENT" verify "$tmp" || rc=1
  if [ "$target" = public ]; then
    # The explorer shows a block once it is finalized on the Logos Blockchain (about an hour after the run).
    tail -n +2 "$RECORD/txs.tsv" | while IFS=$'\t' read -r label hash block _; do
      page="$(curl -sS -m 30 -A "$UA" "$EXPLORER/transaction/$hash" || true)"
      bpage="$(curl -sS -m 30 -A "$UA" "$EXPLORER/block/$block" || true)"
      if printf '%s' "$page" | grep -q "$hash" && printf '%s' "$bpage" | grep -q "$hash"; then
        echo "MATCH    explorer: $label ($hash) in block $block"
      else
        echo "MISMATCH explorer: $label ($hash) not shown in block $block"; echo x >> "$tmp/explorer-bad"
      fi
    done
    [ -f "$tmp/explorer-bad" ] && rc=1
  fi
  rm -rf "$tmp"
  [ $rc -eq 0 ] && echo "verify: all verdicts match the record ($RECORD)" || echo "verify: MISMATCH (see above)"
  return $rc
}

local_run() {
  need_client
  [ -x "$LEZ_BIN/sequencer_service" ] || { echo "build first: scripts/build.sh --lez" >&2; exit 2; }
  ensure_wallet
  local home="$STATE/sequencer"
  if [ ! -f "$home/config.json" ]; then
    mkdir -p "$home"
    python3 -I - "$SPIKE/upstream/lez-v0.3.1/lez/sequencer/service/configs/debug/sequencer_config.json" "$home" "$PAYER" <<'EOF'
import json, sys
c = json.load(open(sys.argv[1]))
c["home"] = sys.argv[2]
c["block_create_timeout"] = "10s"
c["genesis"] = [{"supply_account": {"account_id": sys.argv[3], "balance": 10**15}}]
c.pop("gossip", None)  # one sequencer: no gossip network (its QUIC transport is not available in every sandbox)
json.dump(c, open(sys.argv[2] + "/config.json", "w"), indent=1)
EOF
  fi
  echo "== starting LEZ v0.3.1's sequencer_service (standalone: mock Bedrock) at $URL"
  ( cd "$home" && RUST_LOG=info,kameo=warn nohup "$LEZ_BIN/sequencer_service" "$home/config.json" --port 3040 \
      > "$STATE/sequencer.log" 2>&1 & echo $! > "$STATE/sequencer.pid" )
  for _ in $(seq 60); do rpc getLastBlockId '[]' 2>/dev/null | grep -q result && break; sleep 2; done
  echo "sequencer up: $(rpc getLastBlockId '[]')"
  trap 'kill "$(cat "$STATE/sequencer.pid")" 2>/dev/null || true' EXIT
  FERRATA_SEQUENCER="$URL" FERRATA_TESTNET_SEND=yes run
  FERRATA_SEQUENCER="$URL" verify
  grep -c -E 'Created block|block_id' "$STATE/sequencer.log" > /dev/null || true
  cp "$STATE/sequencer.log" "$RECORD/sequencer.log.txt" 2>/dev/null || true
  python3 -I - "$home/config.json" "$RECORD/sequencer_config.json" <<'EOF'
import json, sys
c = json.load(open(sys.argv[1])); c["home"] = "<state>/sequencer"
json.dump(c, open(sys.argv[2], "w"), indent=1)
EOF
}

case "$cmd" in
  plan) plan "$@" ;;
  run) run ;;
  verify) verify ;;
  local) local_run ;;
  *) sed -n '2,20p' "$0"; exit 2 ;;
esac
