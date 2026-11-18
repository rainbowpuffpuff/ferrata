#!/usr/bin/env bash
# Ferrata's light client on LEZ v0.3.1, the public LEZ testnet's version (EXPERIMENTAL, UNAUDITED: README.md).
#
#   ./testnet.sh plan [--fresh]   every transaction `run` sends, signed for this payer and simulated here on LEZ v0.3.1's
#                                 state machine as its block builder will group them: its block, size, declared and
#                                 simulated execution gas, fee; the funds the payer needs and how the LEZ docs say to
#                                 get them. Makes the payer's fresh key (outside the repository) if there is none.
#                                 Sends nothing. Uses the committed local run's data, or with --fresh a new checkpoint,
#                                 the next two finality updates and the vault's proofs (waits about 13 min).
#   ./testnet.sh run              fetch a fresh Sepolia checkpoint, two newer finality updates and the vault's storage
#                                 proofs; sign everything (explicit nonces, measured gas limits); queue it at once; wait.
#                                 Expected: block A = deployment (7 transactions) + Init; block B = finality update 1;
#                                 block C = finality update 2 + the attestation. Resumable and idempotent: data fetched
#                                 and transactions signed are kept and never rebuilt; a second run only re-sends the same
#                                 signed bytes (harmless: a duplicate is dropped at the nonce check) and waits.
#                                 Refuses to start unless FERRATA_TESTNET_SEND=yes.
#   ./testnet.sh resend           send the signed transactions the chain does not hold yet again, byte for byte (after a
#                                 rewind or a sequencer restart).
#   ./testnet.sh verify           re-read every verdict from the sequencer's state and compare with the committed record;
#                                 for the public testnet also the explorer, PENDING while a block is Pending on
#                                 Bedrock. Exit 0 only if all match (3 if only explorer checks are pending).
#   ./testnet.sh local            the same run against a LEZ v0.3.1 sequencer started here (standalone: mock Bedrock),
#                                 then verify; records under results/local/.
#
# Target: the public testnet (https://testnet.lez.logos.co) unless FERRATA_SEQUENCER is set. Keys and run data live under
# $FERRATA_STATE (default spike/tmp/testnet-v03/<public|local>), never in the repository; the record of a run (no keys)
# is copied to results/<public|local>/. Build first: scripts/build.sh --lez.
set -euo pipefail
cd "$(dirname "$0")"
HERE="$PWD"
SPIKE="$(cd ../.. && pwd)"
PUBLIC_URL="https://testnet.lez.logos.co"
EXPLORER="https://explorer.testnet.lez.logos.co"
VAULT="0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c"   # FerrataVault v5 on Sepolia (docs/sepolia.md)
VAULT_SLOTS="0,1"                                      # depositRoot, nextLeafIndex (as the v0.4 bridge's ImportRoot reads them)
CLIENT="$HERE/target/release/ferrata-v03"
LEZ_BIN="$HERE/target/lez/release"
UA="curl/8"
WAIT_SECONDS="${FERRATA_WAIT:-1800}"

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

need_client() {
  [ -x "$CLIENT" ] || { echo "build first: scripts/build.sh --lez" >&2; exit 2; }
  { "$CLIENT" 2>&1 || true; } | grep -q 'usage' || { echo "$CLIENT is not the ferrata-v03 client: scripts/build.sh --lez" >&2; exit 2; }
}

rpc() { # rpc METHOD PARAMS_JSON: one read-only JSON-RPC call to the sequencer
  curl -sS -m 30 -A "$UA" -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":$2}" "$URL"
}

ensure_wallet() { # the payer, the program's header account and its segment accounts: fresh keys outside the repository
  mkdir -p "$RUN"
  if [ ! -f "$LEE_WALLET_HOME_DIR/storage.json" ]; then
    "$CLIENT" new-wallet "$RUN" "$URL" > "$STATE/payer"
    echo "fresh keys were made under $LEE_WALLET_HOME_DIR (never in the repository)"
  fi
  PAYER="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["payer"])' "$RUN/state.json")"
  [ -n "$PAYER" ] || { echo "no payer account in $RUN/state.json" >&2; exit 1; }
}

balance() {
  rpc getAccountBalance "[\"$PAYER\"]" 2>/dev/null | python3 -I -c 'import json,sys
try: print(json.load(sys.stdin).get("result", "?"))
except Exception: print("?")'
}

last_slot() { # the finalized slot of the newest fetched update, else the checkpoint's
  local d="$1" s
  s="$(ls "$d/updates" 2>/dev/null | sed -n 's/.*-finality-\([0-9]*\)\.ssz/\1/p' | sort -n | tail -1)"
  [ -n "$s" ] && echo "$s" || python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["bootstrap_slot"])' "$d/chain.json"
}

fetch_all() { # DIR: a checkpoint, two newer finality updates, the vault's proofs under the newest one's header
  local d="$1" f slot hash n
  [ -f "$d/chain.json" ] || python3 -I scripts/fetch.py checkpoint "$d"
  n="$(ls "$d/updates" 2>/dev/null | grep -c -- '-finality-' || true)"
  while [ "$n" -lt 2 ]; do
    python3 -I scripts/fetch.py finality "$d" "$(last_slot "$d")" 1500
    n=$((n + 1))
  done
  f="$(ls "$d/updates" | grep -- '-finality-' | sort | tail -1)"
  slot="$(last_slot "$d")"
  hash="$(python3 -I - "$d/updates/$f" <<'EOF'
import sys
b = open(sys.argv[1], "rb").read()
H = 112 + 32 + 11 * 32   # a Gloas LightClientHeader; the finalized header follows the attested one
print("0x" + b[H + 112:H + 144].hex())
EOF
)"
  [ -f "$d/proofs/$slot/proof.json" ] || python3 -I scripts/fetch.py proof "$d" "$slot" "$hash" "$VAULT" "$VAULT_SLOTS"
}

copy_record() { # the run's record, without keys, into the repository's results
  mkdir -p "$RECORD"
  for f in chain.json state.json applied.txt signed.tsv nonces.json txs.tsv record.json requests.tsv batch.json bootstrap.ssz; do
    [ -f "$RUN/$f" ] && cp "$RUN/$f" "$RECORD/$f"
  done
  for d in updates proofs signed; do
    rm -rf "$RECORD/$d"; [ -d "$RUN/$d" ] && cp -r "$RUN/$d" "$RECORD/$d"
  done
  sed -i "s|$RUN/|<run>/|g" "$RECORD/batch.json" 2>/dev/null || true
  echo "record: $RECORD"
}

plan() {
  need_client
  ensure_wallet
  local data="$STATE/plan-data"
  rm -rf "$data"; mkdir -p "$data"
  if [ "${1:-}" = --fresh ]; then
    fetch_all "$data"
  else
    [ -f "$HERE/results/local/chain.json" ] || { echo "no committed local run: ./testnet.sh local first, or plan --fresh" >&2; exit 2; }
    for f in chain.json bootstrap.ssz; do cp "$HERE/results/local/$f" "$data/"; done
    cp -r "$HERE/results/local/updates" "$HERE/results/local/proofs" "$data/"
  fi
  cp "$RUN/state.json" "$data/state.json"
  echo "== plan: the run's transactions, signed for payer $PAYER and simulated on LEZ v0.3.1's state machine"
  echo "   (data: $([ "${1:-}" = --fresh ] && echo "fresh, fetched now" || echo "the committed local run's, results/local"))"
  "$CLIENT" queue "$data" --dry-run | tee "$STATE/plan.txt"
  cat <<EOF

== the target
sequencer: $URL
fee state now: $(rpc getFeeState '[]')
head: $(rpc getLastBlockId '[]')
payer (fresh key under $LEE_WALLET_HOME_DIR): $PAYER, balance now $(balance)
program account (chosen before deployment): $(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["program"])' "$RUN/state.json")

== funds
The payer pays every fee. For each transaction the sequencer reserves gas_limit x base_fee_exec + bytes x base_fee_stor
before running it and refunds what it does not charge in the same transaction; the payer must hold the largest reserve
on top of the fees so far. Fund the payer with at least 150,000,000 native units (the plan's fees, about 21M at base
fee 8, plus the largest reserve at a doubled base fee, with margin).
How the LEZ docs say to get testnet funds (logos-docs, docs/lez/get-started/run-lez-wallet-via-cli.md, "Fund the sender
account"): bridge tokens from the Logos Blockchain with a channel deposit (Basecamp; in Metadata, the payer's account id
without its Public/ prefix), or have another testnet user transfer native tokens to it. This script never asks a faucet
and never funds anything: the owner does.
The same sequence for testnet-ops's queued sender: $data/batch.dry-run.json (its "elf" path: $data/ferrata_v03.bin).
EOF
}

run() {
  need_client
  if [ "${FERRATA_TESTNET_SEND:-}" != yes ]; then
    echo "refusing to send: set FERRATA_TESTNET_SEND=yes to run against $URL" >&2; exit 1
  fi
  ensure_wallet
  echo "== run against $URL, payer $PAYER (balance $(balance)), $(date -u +%FT%TZ)"
  if [ ! -f "$RUN/signed.tsv" ]; then
    fetch_all "$RUN"
    "$CLIENT" queue "$RUN"
  else
    "$CLIENT" queue "$RUN"            # signs only what is not signed yet (nothing, after a complete first run)
    "$CLIENT" resend "$RUN"
  fi
  if "$CLIENT" wait "$RUN" "$WAIT_SECONDS"; then
    "$CLIENT" record "$RUN"
    copy_record
    echo "== done: payer balance $(balance), $(date -u +%FT%TZ)"
  else
    copy_record
    echo "== not all included yet: ./testnet.sh run again later (it re-sends the same signed bytes and waits)" >&2
    exit 3
  fi
}

verify() {
  need_client
  [ -f "$RECORD/record.json" ] || { echo "no record at $RECORD" >&2; exit 2; }
  local tmp rc=0 pending=0
  tmp="$(mktemp -d "$STATE/verify.XXXXXX")"
  cp -r "$RECORD/." "$tmp/"
  "$CLIENT" verify "$tmp" || rc=1
  if [ "$target" = public ]; then
    # The explorer shows a block once it is final on the Logos Blockchain; until then the block is Pending over RPC.
    while IFS=$'\t' read -r hash block status label; do
      if [ "$block" = "-" ]; then echo "MISMATCH explorer: $label ($hash) is in no block"; rc=1; continue; fi
      if [ "$status" = Pending ]; then echo "PENDING  explorer: $label ($hash) in block $block, Pending on Bedrock"; pending=1; continue; fi
      if curl -sS -m 30 -A "$UA" "$EXPLORER/transaction/$hash" | grep -q "$hash" \
         && curl -sS -m 30 -A "$UA" "$EXPLORER/block/$block" | grep -q "$hash"; then
        echo "MATCH    explorer: $label ($hash) in block $block ($status)"
      else
        echo "MISMATCH explorer: $label ($hash) not shown in block $block ($status)"; rc=1
      fi
    done < <("$CLIENT" status "$tmp")
  fi
  rm -rf "$tmp"
  if [ $rc -ne 0 ]; then echo "verify: MISMATCH (see above)"; return 1; fi
  if [ $pending -ne 0 ]; then echo "verify: state matches; explorer PENDING until the blocks are final on Bedrock"; return 3; fi
  echo "verify: every verdict matches the record ($RECORD)"
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
c.pop("gossip", None)  # one sequencer: no gossip network (its QUIC listener fails on a Modal box, os error 95)
json.dump(c, open(sys.argv[2] + "/config.json", "w"), indent=1)
EOF
  fi
  # Fetch everything first (about 13 min of Sepolia finality), so that the whole sequence is queued at once, as on
  # the public testnet, where blocks come hours apart.
  fetch_all "$RUN"
  echo "== starting LEZ v0.3.1's sequencer_service (standalone: mock Bedrock) at $URL, $(date -u +%FT%TZ)"
  ( cd "$home" && RUST_LOG=info,kameo=warn nohup "$LEZ_BIN/sequencer_service" "$home/config.json" --port 3040 \
      > "$STATE/sequencer.log" 2>&1 & echo $! > "$STATE/sequencer.pid" )
  trap 'kill "$(cat "$STATE/sequencer.pid")" 2>/dev/null || true' EXIT
  for _ in $(seq 90); do rpc getLastBlockId '[]' 2>/dev/null | grep -q result && break; sleep 2; done
  echo "sequencer up: $(rpc getLastBlockId '[]')"
  FERRATA_SEQUENCER="$URL" FERRATA_TESTNET_SEND=yes run
  FERRATA_SEQUENCER="$URL" verify
  grep -E 'deferred|Nonce mismatch|Created block|block [0-9]+ ' "$STATE/sequencer.log" | tail -n 200 > "$RECORD/sequencer-excerpt.log" || true
  python3 -I - "$home/config.json" "$RECORD/sequencer_config.json" <<'EOF'
import json, sys
c = json.load(open(sys.argv[1])); c["home"] = "<state>/sequencer"
json.dump(c, open(sys.argv[2], "w"), indent=1)
EOF
}

case "$cmd" in
  plan) plan "$@" ;;
  run) run ;;
  resend) need_client; ensure_wallet; "$CLIENT" resend "$RUN" ;;
  verify) verify ;;
  local) local_run ;;
  *) sed -n '2,29p' "$0"; exit 2 ;;
esac
