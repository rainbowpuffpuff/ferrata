#!/usr/bin/env bash
# ferrata_msg_v2 end to end on a local LEZ v0.4.0-rc3 sequencer (LEZ's own, standalone mode, in-process mock Bedrock,
# on loopback) on a build machine (the Modal build box through mrun). Nothing here reaches a public network.
#
#   box.sh build      LEZ rc3's sequencer_service (feature `standalone`) and LEZ's wallet CLI (../wallet/test/box.sh)
#   box.sh e2e BIN    deploy BIN (ferrata_msg_v2.bin) with LEZ's CLI (program-loader deploy, paid by LEZ's local test
#                     account), fund four fresh keys, then drive ferrata-msg through every rule a user meets: keys,
#                     publish, --expect-key, send, inbox, a stranger's second message (refused before sending, then
#                     forced: reverted in its block), block, contacts-only both ways, delete (its envelope emptied), key
#                     rotation read across epochs, clear, no plaintext in any state. Each outcome is read from its block
#                     (LEZ's RPC serves which transactions reverted, not their exit codes). Record: test/results/local-2/.
#
# Scratch: MSG_WORK (default target/msg-box in the repository, git-ignored). NODE20: optionally a Node 20.x binary
# older than 20.19 (no ES-module detection), to check that the CLI still loads the wallet's lez.js there.
# Everything started is stopped on return.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MSG="$(cd "$HERE/.." && pwd)"
WEB="$(cd "$MSG/.." && pwd)"
REPO="$(cd "$WEB/../../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
export WALLET_WORK="${MSG_WORK:-$REPO/target/msg-box}"
LEZ_SRC="${LEZ_SRC:-$BESIDE/upstream/lez-v0.4.0-rc3}"
BIN="$WALLET_WORK/bin"
RUN="$WALLET_WORK/run"
RPC_PORT="${MSG_RPC_PORT:-3040}"
BEDROCK_FUNDING_PUBLIC_KEY=2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26   # LEZ's Justfile value
OUT="$HERE/results/${MSG_RESULT:-local-2}"
die() { echo "error: $*" >&2; exit 1; }

[ "${1:-}" = build ] && exec bash "$WEB/wallet/test/box.sh" build

rpc() { curl -sS -m 10 -H 'Content-Type: application/json' \
  --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "http://127.0.0.1:$RPC_PORT"; }
SEQ_PID=""
stop_all() { if [ -n "$SEQ_PID" ]; then kill -TERM "$SEQ_PID" 2>/dev/null || true; sleep 1; kill -KILL "$SEQ_PID" 2>/dev/null || true; fi; }

# As ../wallet/test/box.sh start_sequencer.
start_sequencer() {
  [ -x "$BIN/sequencer_service_standalone" ] || die "build first"
  rm -rf "$RUN"; mkdir -p "$RUN/logs"
  python3 -I - "$LEZ_SRC/lez/sequencer/service/networks/local.json" "$RUN/sequencer_config.json" "${1:-}" <<'PY'
import json, sys
src, dst, block_time = sys.argv[1:4]
c = json.load(open(src))
c["gossip"]["listen_addr"] = "/ip4/127.0.0.1/udp/0/quic-v1"
c["metrics_listen_address"] = "127.0.0.1:9000"
if block_time:
    c["block_create_timeout"] = block_time
json.dump(c, open(dst, "w"), indent=2)
PY
  "$BIN/sequencer_service_standalone" setup "$RUN/sequencer_home" --config "$RUN/sequencer_config.json" \
    --funding-public-key "$BEDROCK_FUNDING_PUBLIC_KEY" --create-channel > "$RUN/logs/setup.log" 2>&1 \
    || { tail -30 "$RUN/logs/setup.log"; die "sequencer setup failed"; }
  env -u RISC0_DEV_MODE RUST_LOG="info,risc0_zkvm=warn,kameo=warn" \
    "$BIN/sequencer_service_standalone" start "$RUN/sequencer_home" --rpc-listen-address "127.0.0.1:$RPC_PORT" \
    > "$RUN/logs/sequencer.log" 2>&1 &
  SEQ_PID=$!
  trap stop_all EXIT
  local i head=""
  for i in $(seq 1 180); do
    head="$(rpc getLastBlockId 2>/dev/null | sed -n 's/.*"result":\([0-9]*\).*/\1/p')" || true
    [ -n "$head" ] && [ "$head" -ge 1 ] && break
    kill -0 "$SEQ_PID" 2>/dev/null || { tail -40 "$RUN/logs/sequencer.log"; die "the sequencer exited"; }
    sleep 1
  done
  [ -n "$head" ] || { tail -40 "$RUN/logs/sequencer.log"; die "the sequencer's RPC did not answer"; }
  echo "== sequencer up: head $head, RPC 127.0.0.1:$RPC_PORT"
}
cli_wallet() {
  export LEE_WALLET_HOME_DIR="$RUN/cli-wallet" LEE_WALLET_PASSWORD="local-test-only"
  if [ ! -f "$LEE_WALLET_HOME_DIR/wallet_config.json" ]; then
    mkdir -p "$LEE_WALLET_HOME_DIR"
    sed "s#http://127.0.0.1:3040#http://127.0.0.1:$RPC_PORT#" "$LEZ_SRC/lez/wallet/configs/debug/wallet_config.json" \
      > "$LEE_WALLET_HOME_DIR/wallet_config.json"
  fi
  "$BIN/wallet" "$@"
}
new_public() { cli_wallet account new public 2>&1 | grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1; }

[ "${1:-}" = e2e ] || { sed -n '2,16p' "${BASH_SOURCE[0]}"; exit 2; }
PROG_BIN="${2:?the program binary (ferrata_msg_v2.bin)}"
rm -rf "$OUT"; mkdir -p "$OUT"
exec > >(tee "$OUT/record.txt") 2>&1
echo "== ferrata_msg_v2 e2e, $(date -u +%FT%TZ); binary sha256 $(sha256sum "$PROG_BIN" | cut -d' ' -f1), $(stat -c %s "$PROG_BIN") bytes; node $(node --version)"
sed 's/^/   LEZ /' "$BIN/SHA256SUMS" 2>/dev/null || true
start_sequencer "${MSG_BLOCK_TIME:-2s}"
# LEZ's published local test key A, read from LEZ's own source (never copied into this repository), as
# ../wallet/test/e2e.sh does; it is funded at the standalone sequencer's genesis.
KEY_A="$(python3 -I - "$LEZ_SRC/lez/testnet_initial_state/src/lib.rs" PRIVATE_KEY_PUB_ACC_A <<'PY'
import re, sys
src, name = open(sys.argv[1]).read(), sys.argv[2]
m = re.search(r"const " + name + r": \[u8; 32\] = \[([^\]]*)\]", src)
print(bytes(int(x) for x in m.group(1).replace("\n", " ").split(",") if x.strip()).hex())
PY
)"
cli_wallet account list > "$RUN/logs/cli-init.log" 2>&1 || true
A="$(cli_wallet account import public --private-key "$KEY_A" 2>&1 | grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1)"
[ -n "$A" ] || die "could not import LEZ's local test account A"

# --- deploy: header + one segment per 96 KiB of the program's user ELF (program_loader_core::MAX_SEGMENT_DATA_LEN; the
# binary also holds the kernel, so its size over-counts): try the binary's count, then the count LEZ's CLI names.
size="$(stat -c %s "$PROG_BIN")"; nseg=$(( (size + 98303) / 98304 ))
t0=$(date +%s)
for attempt in 1 2; do
  H="$(new_public)"; SEGS=(); for _ in $(seq 1 "$nseg"); do SEGS+=("$(new_public)"); done
  if cli_wallet program-loader deploy --elf "$PROG_BIN" --header "$H" --segments "${SEGS[@]}" --immutable --payer "$A" \
    > "$RUN/logs/deploy.txt" 2>&1; then break; fi
  want="$(sed -n 's/.*splits into \([0-9]*\) segment.*/\1/p' "$RUN/logs/deploy.txt" | head -1)"
  [ "$attempt" = 1 ] && [ -n "$want" ] || { tail -5 "$RUN/logs/deploy.txt"; die "deploy failed"; }
  echo "== LEZ's CLI: the program splits into $want segments, not $nseg"
  nseg="$want"
done
PROGRAM="$(grep -o -E 'Header account id: *[1-9A-HJ-NP-Za-km-z]{32,44}' "$RUN/logs/deploy.txt" | awk '{print $NF}')"
PROGRAM="${PROGRAM#Public/}"
echo "== deployed in $(( $(date +%s) - t0 )) s by LEZ's CLI (immutable): $nseg segments + header; program $PROGRAM"
grep -E "included in block|Program deployed" "$RUN/logs/deploy.txt" | cut -c1-120 > "$OUT/deploy.txt"

# --- four fresh keys, funded by the test account (as the faucet would)
cd "$MSG"
export FERRATA_RPC="http://127.0.0.1:$RPC_PORT" FERRATA_MSG_PROGRAM="$PROGRAM"
KEYS="$WALLET_WORK/keys"; mkdir -p "$KEYS"
amount=1000000000000
for who in alice bob carol dave; do
  node -e 'process.stdout.write(require("crypto").randomBytes(32).toString("hex"))' > "$KEYS/$who.hex"
  read -r PK ID < <(node --no-warnings --input-type=module -e "import * as l from '../wallet/lez.js'; import {readFileSync} from 'node:fs'; const n=l.rawKeyNode(readFileSync('$KEYS/$who.hex','utf8')); console.log(l.hex(n.pk), l.idToString(n.accountId))")
  eval "${who^^}=$ID"
  cli_wallet auth-transfer send --from "$A" --to-pk "$PK" --amount "$amount" > "$RUN/logs/fund-$who.txt" 2>&1 || { cat "$RUN/logs/fund-$who.txt"; die "funding $who failed"; }
done
sleep 6

PASS=0; FAIL=0
check() { if eval "$2"; then PASS=$((PASS + 1)); echo "  PASS $1"; else FAIL=$((FAIL + 1)); echo "  FAIL $1"; fi; }
bal() { rpc getAccountBalance "[\"$1\"]" | sed -n 's/.*"result":"\{0,1\}\([0-9]*\).*/\1/p'; }
LAST=""; CODE=0
m() { # WHO ARGS...: run the CLI as WHO, keep its output in $LAST and its exit status in $CODE
  local who="$1"; shift
  echo "\$ ferrata-msg $* ($who)"
  CODE=0; LAST="$(node --no-warnings ferrata-msg.mjs "$@" --key "$KEYS/$who.hex" 2>&1)" || CODE=$?
  echo "$LAST" | sed 's/^/    /'; echo "    [exit $CODE]"
}
probe() { node --no-warnings test/probe.mjs "$FERRATA_RPC" "$PROGRAM" "$@"; }
# jq_ EXPR: a Python expression over the JSON on stdin (as d); a tuple prints space-separated.
jq_() { python3 -I -c "import json,sys; d=json.load(sys.stdin); r=($1); print(*(r if isinstance(r, tuple) else (r,)))"; }
entries() { probe "$1" | jq_ 'len(d["inbox"]["entries"])'; }
for who in alice bob carol dave; do id="${who^^}"; echo "== $who ${!id}: balance $(bal "${!id}")"; done

echo; echo "== 1. keys; a send before the recipient has a key is refused before any fee"
m alice keygen
b0="$(bal "$ALICE")"; m alice send "$BOB" "hello before you have a key"
check "send to an account with no key: refused NO_KEY, nothing sent, no fee" '[ $CODE = 1 ] && grep -q NO_KEY <<<"$LAST" && [ "$(bal "$ALICE")" = "$b0" ]'

echo; echo "== 2. bob publishes a hybrid key: his profile and his inbox appear"
m bob publish
check "publish applied in its block" '[ $CODE = 0 ] && grep -q "applied in block" <<<"$LAST"'
FP="$(sed -n 's/.*fingerprint \([1-9A-HJ-NP-Za-km-z]*\)).*/\1/p' <<<"$LAST" | head -1)"
V="$(probe "$BOB" | jq_ 'd["inbox"]["nextN"], len(d["inbox"]["entries"]), d["key"]["hybrid"], d["key"]["epoch"]')"
check "bob's inbox exists, empty, and his key is hybrid epoch 0" '[ "$V" = "0 0 True 0" ]'

INBOX_BOB="$(node --no-warnings --input-type=module -e "import * as M from './ferrata-msg.js'; import * as l from '../wallet/lez.js'; console.log(l.idToString(M.inboxAccount(l.idFromString('$PROGRAM'), l.idFromString('$BOB'))))")"
b0="$(bal "$ALICE")"; m alice send "$INBOX_BOB" "to an inbox account"
check "send to bob's inbox account (one of the program's own accounts): refused BAD_STATE before sending, no fee" '[ $CODE = 1 ] && grep -q BAD_STATE <<<"$LAST" && [ "$(bal "$ALICE")" = "$b0" ]'

echo; echo "== 3. alice sends, checking bob's key fingerprint; bob reads"
b0="$(bal "$ALICE")"; m alice send "$BOB" "should not go" --expect-key 1111111111111
check "--expect-key with another fingerprint: refused before sending, no fee" '[ $CODE = 1 ] && grep -q KEY_MISMATCH <<<"$LAST" && [ "$(bal "$ALICE")" = "$b0" ]'
t0=$(date +%s%3N)
m alice send "$BOB" "Hi Bob, this is Alice: end-to-end encrypted, post-quantum hybrid, padded, on LEZ v0.4, one account per message." --expect-key "$FP"
t1=$(date +%s%3N); FEE_SEND=$(( b0 - $(bal "$ALICE") ))
check "send with bob's fingerprint: applied in its block" '[ $CODE = 0 ] && grep -q "applied in block" <<<"$LAST"'
echo "  measured: send to its block $(( t1 - t0 )) ms (wall clock, the CLI's reads included); fee $FEE_SEND units"
m bob inbox
check "bob's inbox decrypts alice's message" '[ $CODE = 0 ] && grep -q "this is Alice" <<<"$LAST"'

echo; echo "== 4. a stranger's second unread message: refused before sending, then forced: reverted in its block"
m carol send "$BOB" "carol 1"
check "carol (a stranger) sends one: applied" '[ $CODE = 0 ]'
b0="$(bal "$CAROL")"; m carol send "$BOB" "carol 2"
check "carol's second: refused STRANGER_LIMIT before sending, no fee" '[ $CODE = 1 ] && grep -q STRANGER_LIMIT <<<"$LAST" && [ "$(bal "$CAROL")" = "$b0" ]'
n0="$(entries "$BOB")"
m carol send "$BOB" "carol 2, forced past the client's check" --force
FEE_REFUSED=$(( b0 - $(bal "$CAROL") )); n1="$(entries "$BOB")"
check "forced: reverted in its block (the block's reverted list), the fee charged, nothing stored" '[ $CODE = 1 ] && grep -q "reverted in block" <<<"$LAST" && [ $FEE_REFUSED -gt 0 ] && [ "$n1" = "$n0" ]'
echo "  measured: carol's fee for the reverted transaction $FEE_REFUSED units (the exit code, STRANGER_LIMIT, is the harness's: lez/results/messages-v2.md)"

echo; echo "== 5. bob blocks carol"
m bob block "$CAROL"
check "block applied" '[ $CODE = 0 ]'
b0="$(bal "$CAROL")"; m carol send "$BOB" "carol after the block"
check "carol (blocked): refused BLOCKED before sending, no fee" '[ $CODE = 1 ] && grep -q BLOCKED <<<"$LAST" && [ "$(bal "$CAROL")" = "$b0" ]'

echo; echo "== 6. contacts-only, both ways"
m bob contact "$ALICE"
check "bob adds alice as a contact: applied" '[ $CODE = 0 ]'
m bob policy contacts
check "policy contacts: applied" '[ $CODE = 0 ]'
b0="$(bal "$DAVE")"; m dave send "$BOB" "dave, not a contact"
check "dave (not a contact): refused NOT_A_CONTACT before sending, no fee" '[ $CODE = 1 ] && grep -q NOT_A_CONTACT <<<"$LAST" && [ "$(bal "$DAVE")" = "$b0" ]'
n0="$(entries "$BOB")"
m dave send "$BOB" "dave, forced" --force
n1="$(entries "$BOB")"
check "dave forced: reverted in its block, nothing stored" '[ $CODE = 1 ] && grep -q "reverted in block" <<<"$LAST" && [ "$n1" = "$n0" ]'
m alice send "$BOB" "alice, a contact, under contacts-only"
check "alice (a contact) under contacts-only: applied" '[ $CODE = 0 ] && grep -q "applied in block" <<<"$LAST"'

echo; echo "== 7. bob deletes carol's message: its envelope account is emptied"
CN="$(probe "$BOB" | jq_ "[e['n'] for e in d['inbox']['entries'] if e['from']=='$CAROL'][0]")"
m bob delete "$CN"
check "delete applied" '[ $CODE = 0 ] && grep -q "applied in block" <<<"$LAST"'
V="$(probe "$BOB" --envelope "$CAROL:0" | jq_ "[e['held'] for e in d['envelopes'] if e['from']=='$CAROL'], len([e for e in d['inbox']['entries'] if e['from']=='$CAROL'])")"
check "carol's envelope account is empty and her entry gone" '[ "$V" = "[False] 0" ]'

echo; echo "== 8. bob rotates to epoch 1; alice's next message uses it; bob reads both epochs"
m bob publish
check "rotation applied" '[ $CODE = 0 ] && grep -q "epoch 1" <<<"$LAST"'
m alice send "$BOB" "after the rotation, to epoch 1"
check "send to epoch 1: applied" '[ $CODE = 0 ]'
m bob inbox
check "bob reads epoch-0 and epoch-1 messages" '[ $CODE = 0 ] && grep -q "this is Alice" <<<"$LAST" && grep -q "after the rotation" <<<"$LAST"'
node --no-warnings ferrata-msg.mjs inbox --json --key "$KEYS/bob.hex" > "$OUT/bob-inbox.json"

echo; echo "== 9. on chain: ciphertext only"
probe "$BOB" > "$OUT/bob-state.json"
check "no message text in bob's profile, inbox or envelopes" 'python3 -I - "$OUT/bob-state.json" <<'"'"'PY'"'"'
import json, sys
d = json.load(open(sys.argv[1]))
blob = bytes.fromhex(d["profileHex"] + d["inboxHex"] + "".join(e["hex"] for e in d["envelopes"]))
bad = [t for t in (b"Alice", b"rotation", b"carol", b"contact") if t in blob]
print("   ", len(blob), "bytes of state searched; found:", bad)
sys.exit(1 if bad else 0)
PY'

echo; echo "== 10. bob clears everything: every envelope emptied"
LASTN="$(probe "$BOB" | jq_ 'd["inbox"]["entries"][-1]["n"]')"
m bob clear "$LASTN"
check "clear applied" '[ $CODE = 0 ] && grep -q "applied in block" <<<"$LAST"'
V="$(probe "$BOB" --envelope "$ALICE:0" --envelope "$ALICE:1" --envelope "$ALICE:2" | jq_ 'len(d["inbox"]["entries"]), len(d["envelopes"]), any(e["held"] for e in d["envelopes"])')"
check "bob's inbox empty, alice's three envelope accounts empty" '[ "$V" = "0 3 False" ]'

echo; echo "== 11. bulk list edits through the library (the wallet's path): 63 ids added, then 64 removed and 64 added"
BULK="$(node --no-warnings --input-type=module - "$KEYS/bob.hex" "$CAROL" <<'JS'
import { readFileSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
const lez = await import('../wallet/lez.js');
const C = await import('./client.js');
const [keyFile, carol] = process.argv.slice(2);
const node = lez.rawKeyNode(readFileSync(keyFile, 'utf8'));
const seq = new lez.Sequencer(process.env.FERRATA_RPC);
const program = lez.idFromString(process.env.FERRATA_MSG_PROGRAM);
const fresh = (n) => Array.from({ length: n }, () => new Uint8Array(randomBytes(32)));
const first = fresh(63);
const a = await C.block(seq, { node, program, add: first });
const b = await C.block(seq, { node, program, add: fresh(64), remove: [...first, lez.idFromString(carol)] });
const p = await C.profile(seq, program, node.accountId);
console.log(JSON.stringify({ a: { reverted: a.reverted, gas: String(a.gasLimit) }, b: { reverted: b.reverted, gas: String(b.gasLimit) }, blocked: p.blocked.length }));
JS
)"
echo "    $BULK"
check "bulk edits applied within the library's limits (63 ids, then 128), bob's block list full" '[ "$(jq_ "d[\"a\"][\"reverted\"], d[\"b\"][\"reverted\"], d[\"blocked\"]" <<<"$BULK")" = "False False 64" ]'

if [ -n "${NODE20:-}" ]; then
  echo; echo "== 12. the CLI on $("$NODE20" --version) (no ES-module detection before 20.19): it loads the wallet's lez.js"
  CODE=0; LAST="$("$NODE20" ferrata-msg.mjs keygen --key "$KEYS/alice.hex" 2>&1)" || CODE=$?
  echo "$LAST" | sed 's/^/    /'
  check "keygen on Node 20 before 20.19" '[ $CODE = 0 ] && grep -q fingerprint <<<"$LAST"'
fi

echo; echo "== $PASS of $((PASS + FAIL)) checks passed; head $(rpc getLastBlockId)"
[ "$FAIL" = 0 ]
