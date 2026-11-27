#!/usr/bin/env bash
# Ferrata's day-one deployment on LEZ testnet 0.4 (EXPERIMENTAL, UNAUDITED; README.md here). One command, once Logos
# publishes the endpoint:   FERRATA_TESTNET_SEND=yes lez/testnet04/day1.sh go URL
#
#   day1.sh discover URL     read-only: what the endpoint is: health, head, fee state, the program ids it holds, its
#                            channel, the clock (its head block's time against this machine's), whether it is a v0.4
#                            network at all (a method v0.3 lacks), and LEZ's published test accounts' balances there.
#                            Writes STATE/discover.json. Sends no transaction.
#   day1.sh plan URL         every transaction `run` sends, signed (not sent) for this deployment's keys in a scratch
#                            state: sizes, declared gas, maximum fees, the fees at the endpoint's base fees, the funds
#                            the payer and the daemon key need and how to get them. Makes the deployment's fresh keys
#                            (outside the repository, mode 600) if there are none. Sends nothing.
#   day1.sh run URL          refuses unless FERRATA_TESTNET_SEND=yes. Resumable, driven by chain state and by what is
#                            kept beside the keys (KEYS: the signed stages, the instance's checkpoint and id):
#                            (a) deploy the attestation program (image 0x816d1fef…, byte for byte the file
#                            scripts/reproduce-image-ids.sh builds: sha256 checked against docs/image-ids.md), immutable;
#                            (b) `Init` a Sepolia instance from a fresh finalized checkpoint (Lodestar's bootstrap,
#                            its root checked against publicnode's header; the weak-subjectivity bound checked);
#                            (c) start the updater daemon (lez/updater, the public job path) following Sepolia,
#                            detached, with its own key; (d) one attestation of the live Sepolia vault 0x04eE…e44c's
#                            slots 0 and 1 under the instance's newest record; (e) with FERRATA_DAY1_BRIDGE=1, the
#                            bridge program (image 0x2ba8dc18…, since ADR 0048; ADR 0046's 0xffb47f6e… and B1's 0x847a4336… before) and the owner-side Sepolia step for its vault, prepared
#                            (not sent). Every transaction is signed once and kept beside the keys
#                            (KEYS/tx/<stage>/NN.tx); after a restart the same bytes are sent again, never a new
#                            transaction that could apply beside them. The deployments are guarded by the chain too
#                            (their fresh accounts sign at nonce 0); `Init` is signed only from the checkpoint kept in
#                            KEYS/init/ and not at all when its instance exists on chain; the attestation only once per
#                            KEYS. Keep KEYS: with it, a lost STATE (or another spelling of the URL) repeats nothing.
#                            FERRATA_DAY1_UNTIL=a|b|c|d stops after that stage.
#   day1.sh go URL           discover, plan, run, verify, in one command (what day one runs).
#   day1.sh verify URL       re-read from the endpoint: the program's segments and header against the binary, every
#                            stored transaction applied in the block recorded, the instance's id, configuration and
#                            progress, the attestation applied with an event (its event's content is not compared: the
#                            verdict computed natively when it was signed is compared with eth_getProof's values), the
#                            instance's finalized header against publicnode's canonical one. Exit 0 if all match, 1 if
#                            one does not, 3 if a check could not be made (publicnode not answering).
#   day1.sh status URL       the stored transactions' states and the daemon's /health
#   day1.sh daemon-status URL   the daemon's process and /health (URL, FERRATA_DAY1_URL or FERRATA_DAY1_STATE names
#                            the run; without one it refuses and lists the daemons running beside the repository)
#   day1.sh daemon-stop URL  SIGTERM, then wait FERRATA_DAY1_STOP_SECONDS (default 120) and SIGKILL: a transaction the
#                            daemon was settling then has no journal row (the chain holds it); wait longer in a stall
#
# Environment: FERRATA_DAY1_BIN (ferrata-day1 and ferrata-updater, built against the network's LEZ tag:
#   README.md "Build"), FERRATA_DAY1_PROGRAMS (ferrata_lez.bin, ferrata_bridge.bin), FERRATA_DAY1_STATE (run data),
#   FERRATA_DAY1_KEYS (keys), all beside the repository by default (tmp/testnet04/, or FERRATA_DAY1_HOME), never
#   inside it;
#   FERRATA_DAY1_BRIDGE=1 adds stage (e); FERRATA_FOLLOWER_CROSS_ZONE=1 for a network whose programs include the
#   cross-zone ones (discover says so); FERRATA_DAY1_NODE: a Logos Blockchain node's API, for discover.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BESIDE="$(cd "$REPO/.." && pwd)"
UA="curl/8"                                             # a generic User-Agent: nothing about who asks
VAULT="0x04eE8FF19cC11CEE4aE65d03e12b9888043de44c"      # FerrataVault v5 on Sepolia (docs/sepolia.md)
VAULT_SLOTS="0,1"                                       # depositRoot, nextLeafIndex
PROGRAM_SHA256="ddfde69bea89a5b882f55266fcb4b7c887df7debc1c9d4972d8ebea5a72eaad9"   # docs/image-ids.md
PROGRAM_IMAGE="816d1fef464d25493c5bacf6e97dea288419821bc6ab052f9dd7ed29ee4f03df"
BRIDGE_SHA256="9cd9d3573562cb5a8452e413ee17b402bbf963d753e2c55294708a23c29d2aed"   # docs/image-ids.md ferrata_bridge_lezfee (since ADR 0048, 27 Nov 2026; ADR 0046's and B1's before)
BRIDGE_IMAGE="2ba8dc18461fc8a62895fcf370855dcb1427d84f888659a7d52102af4ed817fb"
FETCH_PY="$REPO/lez-v03/scripts/fetch.py"               # public beacon and execution APIs, generic User-Agent
T04="${FERRATA_DAY1_HOME:-$BESIDE/tmp/testnet04}"           # the day-one kit's own folder (tests use one of theirs)
TOOL_DIR="${FERRATA_DAY1_BIN:-$T04/bin}"
PROGRAMS="${FERRATA_DAY1_PROGRAMS:-$T04/final}"
RETRIES="${FERRATA_DAY1_RETRIES:-40}"                   # a stage that fails (the endpoint down, a restart) is tried again
RETRY_SECONDS="${FERRATA_DAY1_RETRY_SECONDS:-15}"
DAEMON_PORT="${FERRATA_DAY1_DAEMON_PORT:-9464}"

cmd="${1:-}"; shift || true
die() { echo "day1: $*" >&2; exit 1; }
say() { echo "== $(date -u +%FT%TZ) $*"; }
case "$cmd" in
  discover|plan|run|go|verify|status) URL="${1:-}"; [ -n "$URL" ] || die "usage: day1.sh $cmd URL" ;;
  daemon-status|daemon-stop) URL="${1:-${FERRATA_DAY1_URL:-}}"
    if [ -z "$URL" ] && [ -z "${FERRATA_DAY1_STATE:-}" ]; then
      for p in "$T04"/state/*/daemon/pid; do
        [ -f "$p" ] && kill -0 "$(cat "$p")" 2>/dev/null && echo "running: pid $(cat "$p"), $(sed -n 's/^rpc = "\(.*\)"$/URL \1/p' "$(dirname "$p")/updater.toml" 2>/dev/null) ($(dirname "$(dirname "$p")"))" >&2
      done
      die "usage: day1.sh $cmd URL (the URL run was given; or FERRATA_DAY1_URL, or FERRATA_DAY1_STATE): without it the daemon's state is not known"
    fi ;;
  *) sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
slug() { printf '%s' "$1" | sed -E 's#^[a-z]+://##; s#[^A-Za-z0-9.-]+#_#g; s#_+$##'; }
NAME="$(slug "${URL:-none}")"
STATE="${FERRATA_DAY1_STATE:-$T04/state/$NAME}"
KEYS="${FERRATA_DAY1_KEYS:-$T04/keys/$NAME}"
FETCH="$STATE/fetch"
mkdir -p "$STATE"
TOOL="$TOOL_DIR/ferrata-day1"
UPDATER="$TOOL_DIR/ferrata-updater"
# The tool reaches a sequencer that is not on this machine only for the URL named here, and sends to it only when the
# owner set FERRATA_TESTNET_SEND=yes (lez/live/driver/src/live.rs, check_loopback).
export FERRATA_SEQUENCER_REMOTE="$URL"

rpc() { # rpc METHOD PARAMS_JSON: one read-only JSON-RPC call
  curl -sS -m 30 -A "$UA" -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":$2}" "$URL"
}
need_tools() {
  [ -x "$TOOL" ] && [ -x "$UPDATER" ] || die "no ferrata-day1 / ferrata-updater in $TOOL_DIR: build them (README.md, Build)"
}
check_programs() { # the binaries are the final programs of docs/image-ids.md, byte for byte
  echo "$PROGRAM_SHA256  $PROGRAMS/ferrata_lez.bin" | sha256sum -c --quiet - || die "$PROGRAMS/ferrata_lez.bin is not the final attestation program"
  if [ "${FERRATA_DAY1_BRIDGE:-0}" = 1 ]; then
    echo "$BRIDGE_SHA256  $PROGRAMS/ferrata_bridge.bin" | sha256sum -c --quiet - || die "$PROGRAMS/ferrata_bridge.bin is not this tree's bridge program (docs/image-ids.md ferrata_bridge_lezfee, since ADR 0048; B1's is the test network's, one built before B1 the live runs')"
  fi
  if command -v r0vm > /dev/null; then
    [ "$(r0vm --elf "$PROGRAMS/ferrata_lez.bin" --id | tail -n 1)" = "$PROGRAM_IMAGE" ] || die "image id of ferrata_lez.bin"
    [ "${FERRATA_DAY1_BRIDGE:-0}" != 1 ] || [ "$(r0vm --elf "$PROGRAMS/ferrata_bridge.bin" --id | tail -n 1)" = "$BRIDGE_IMAGE" ] || die "image id of ferrata_bridge.bin"
  fi
}
bins() { echo --bin "$PROGRAMS/ferrata_lez.bin" $([ "${FERRATA_DAY1_BRIDGE:-0}" = 1 ] && echo --bridge-bin "$PROGRAMS/ferrata_bridge.bin"); }
tool() { "$TOOL" "$@" --rpc "$URL"; }
until_stage() { # until_stage X: stop here when FERRATA_DAY1_UNTIL=X (a test's partial run)
  if [ "${FERRATA_DAY1_UNTIL:-}" = "$1" ]; then say "stopping after ($1): FERRATA_DAY1_UNTIL=$1"; exit 0; fi
}
retry() { # retry LABEL COMMAND...: a stage that fails is tried again (the endpoint restarting, a rewind); its stored
  # bytes are what is sent again
  local label="$1"; shift; local i
  for i in $(seq 1 "$RETRIES"); do
    if "$@"; then return 0; fi
    say "$label failed (attempt $i of $RETRIES); again in $RETRY_SECONDS s"
    sleep "$RETRY_SECONDS"
  done
  die "$label failed $RETRIES times"
}

# ---- discover ----------------------------------------------------------------------------------------------------
discover() {
  say "discover $URL"
  local rc=0
  python3 -I - "$URL" "$UA" "$STATE/discover.json" "${FERRATA_DAY1_NODE:-}" "$HERE/reference-rc3.json" <<'EOF' || rc=$?
import base64, json, struct, sys, time, urllib.request, urllib.error
url, ua, out, node, ref_path = sys.argv[1:6]
STATUS = ["Pending", "Safe", "Finalized"]   # common::block::BedrockStatus (lez/common/src/block.rs:80-84 at rc3)
def block_fields(r):
    """A block as getBlock serves it: base64 of its borsh (rc1-rc3: the header's block_id u64, prev_block_hash,
    hash, timestamp u64 ms first; the Bedrock status, a one-byte enum, last). A JSON object is read as it is."""
    if isinstance(r, dict):
        h = r.get("header", {})
        return {"timestamp": h.get("timestamp"), "hash": h.get("hash"), "bedrock_status": r.get("bedrock_status")}
    if isinstance(r, str):
        b = base64.b64decode(r)
        return {"block_id": struct.unpack("<Q", b[0:8])[0], "hash": b[40:72].hex(), "timestamp": struct.unpack("<Q", b[72:80])[0],
                "bedrock_status": STATUS[b[-1]] if b[-1] < len(STATUS) else f"tag {b[-1]}", "bytes": len(b)}
    return {"unreadable": repr(r)[:80]}
def call(method, params=None):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params if params is not None else []}).encode()
    req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json", "User-Agent": ua})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        return {"http_error": e.code}
    except Exception as e:
        return {"transport_error": str(e)}
d = {"url": url, "read_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
for m in ["checkHealth", "getLastBlockId", "getFeeState", "getProgramIds", "getChannelId"]:
    d[m] = call(m)
head = d["getLastBlockId"].get("result")
blocks = {}
if isinstance(head, int):
    for b in sorted({max(head - 30, 1), head}):
        try:
            blocks[b] = block_fields(call("getBlock", [b]).get("result"))
        except Exception as e:
            blocks[b] = {"unreadable": str(e)}
d["blocks"] = blocks
# v0.4 has getPublications (lez/sequencer/service/rpc/src/lib.rs at v0.4.0-rc1..rc3); v0.3.1 answers "method not found".
probe = call("getPublications", [])
code = (probe.get("error") or {}).get("code")
d["v04_probe"] = {"method": "getPublications", "answer": probe.get("error") or ("result" if "result" in probe else probe)}
d["is_v04"] = code != -32601 and "transport_error" not in probe and "http_error" not in probe
progs = d["getProgramIds"].get("result") or {}
d["cross_zone_programs"] = sorted(k for k in progs if "cross_zone" in k or "bridge_lock" in k or "inbox" in k)
try:
    ref = json.load(open(ref_path))
    rp = ref.get("program_ids", {})
    d["builtins_vs_rc3"] = {"same": progs == rp, "only_here": sorted(set(progs) - set(rp)),
                            "only_rc3": sorted(set(rp) - set(progs)),
                            "different_ids": sorted(k for k in set(progs) & set(rp) if progs[k] != rp[k])}
except FileNotFoundError:
    d["builtins_vs_rc3"] = "no reference"
ts = [blocks[b]["timestamp"] for b in sorted(blocks) if blocks[b].get("timestamp")]
if ts:
    d["clock"] = {"head_block_ms": ts[-1], "this_machine_ms": int(time.time() * 1000), "behind_s": round((time.time() * 1000 - ts[-1]) / 1000, 1)}
    if len(ts) == 2 and len(blocks) == 2:
        b0, b1 = sorted(blocks)
        d["clock"]["mean_block_s"] = round((ts[1] - ts[0]) / 1000 / max(b1 - b0, 1), 2)
if node:
    try:
        with urllib.request.urlopen(urllib.request.Request(node.rstrip("/") + "/cryptarchia/info", headers={"User-Agent": ua}), timeout=15) as r:
            d["node_info"] = json.loads(r.read())
    except Exception as e:
        d["node_info"] = {"error": str(e)}
json.dump(d, open(out, "w"), indent=1)
print(f"head block {head}; health {d['checkHealth'].get('result', d['checkHealth'].get('error'))}")
print(f"fee state {json.dumps(d['getFeeState'].get('result'))}")
print(f"channel {d['getChannelId'].get('result')}")
print(f"programs {len(progs)}: {', '.join(sorted(progs))}")
print(f"v0.4 network (getPublications known): {d['is_v04']}; cross-zone programs: {d['cross_zone_programs'] or 'none'}")
print(f"builtin programs against LEZ v0.4.0-rc3's: {json.dumps(d['builtins_vs_rc3'])}")
print(f"clock: {json.dumps(d.get('clock'))}")
print(f"blocks read: {json.dumps(blocks)}")
if not d["is_v04"]:
    sys.exit(3)
EOF
  [ "$rc" = 0 ] || { say "discover: not a v0.4 network, or not readable (exit $rc)"; return "$rc"; }
  if [ -x "$TOOL" ] && [ -d "$KEYS" ]; then
    "$TOOL" inspect --keys "$KEYS" --state "$STATE" $(bins) --rpc "$URL" > "$STATE/inspect-discover.json" 2> "$STATE/inspect-discover.err" \
      && say "this deployment's accounts there: $(python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); print("payer", d["payer"]["account"], "balance", d["payer"]["balance"], "; daemon", d["daemon"]["account"], "balance", d["daemon"]["balance"])' "$STATE/inspect-discover.json")" \
      || say "the tool could not read this endpoint (another LEZ version than it was built for?): $(tail -n 1 "$STATE/inspect-discover.err")"
  fi
}

# ---- keys, checkpoint ------------------------------------------------------------------------------------------------
ensure_keys() {
  if [ ! -f "$KEYS/payer.key" ]; then
    [ "${1:-}" = make ] || [ "${FERRATA_DAY1_NEW_KEYS:-}" = yes ] \
      || die "no keys at $KEYS: \`day1.sh plan\` makes them (a missing key directory must not become a new payer silently)"
    umask 077
    say "fresh keys at $KEYS"
    "$TOOL" keygen --keys "$KEYS" $(bins) > /dev/null
  fi
  chmod 700 "$KEYS"; find "$KEYS" -name '*.key' -exec chmod 600 {} +
}
fetch_checkpoint() { # FETCH/chain.json and bootstrap.ssz: the newest finalized Sepolia checkpoint, two providers agreeing
  [ -f "$1/chain.json" ] && [ -f "$1/bootstrap.ssz" ] && return 0
  say "fetching a fresh finalized Sepolia checkpoint (Lodestar's bootstrap, publicnode's header at its slot)"
  python3 -I "$FETCH_PY" checkpoint "$1"
}

# ---- plan --------------------------------------------------------------------------------------------------------
plan() {
  need_tools; check_programs; discover || true
  ensure_keys make
  local P="$STATE/plan"; rm -rf "$P"; mkdir -p "$P"
  fetch_checkpoint "$P/fetch"
  say "signing (not sending) every stage that can be signed now, in a scratch state ($P)"
  tool sign --keys "$KEYS" --state "$P" --store "$P" --stage deploy-program --bin "$PROGRAMS/ferrata_lez.bin" | tee "$P/deploy-program.txt"
  if [ "${FERRATA_DAY1_BRIDGE:-0}" = 1 ]; then
    tool sign --keys "$KEYS" --state "$P" --store "$P" --stage deploy-bridge --bin "$PROGRAMS/ferrata_bridge.bin" | tee "$P/deploy-bridge.txt"
  fi
  tool sign --keys "$KEYS" --state "$P" --store "$P" --stage init --fetch "$P/fetch" | tee "$P/init.txt"
  python3 -I - "$P" "$KEYS/accounts.json" "$REPO" <<'EOF' | tee "$STATE/plan.txt"
import json, os, sys, glob
P, accounts, repo = sys.argv[1:4]
acc = json.load(open(accounts))
rows, total, reserve = [], 0, 0
fee_state = None
for stage in ["deploy-program", "deploy-bridge", "init"]:
    d = os.path.join(P, "tx", stage)
    if not os.path.isdir(d):
        continue
    st = json.load(open(os.path.join(d, "stage.json")))
    fee_state = st["fee_state"]
    for f in sorted(glob.glob(os.path.join(d, "[0-9][0-9].json"))):
        m = json.load(open(f))
        exec_fee = m["gas_limit"] * fee_state[0]
        stor_fee = m["bytes"] * fee_state[1]
        total += exec_fee + stor_fee
        reserve = max(reserve, int(m["max_fee"]))
        rows.append((stage, m["label"], m["bytes"], m["gas_limit"], exec_fee + stor_fee, int(m["max_fee"])))
# The attestation and the daemon are not signable before the instance exists: their figures are measured elsewhere.
attest_bytes, attest_gas = 7_599, 276_607            # lez/ATTEST-CLI.md: `slot` (storage gas = transaction bytes)
upd_gas, upd_bytes = 9_110_000, 3 * 4_000            # lez/updater/README.md: a finality update, three transactions, 9.11M gas
attest_fee = 2_000_000 * fee_state[0] + 9_000 * fee_state[1]
per_update = upd_gas * fee_state[0] + upd_bytes * fee_state[1]
per_day = 225 * per_update                           # at most one finality update an epoch (6.4 min): 225 a day
print("stage           bytes   gas limit   fee at base fees (if the whole limit is used)   max fee   transaction")
for s, label, b, g, fee, mf in rows:
    print(f"{s:<15} {b:>7} {g:>11} {fee:>16} {mf:>16}   {label}")
print(f"attest          ~{attest_bytes:>6} {2_000_000:>11} {attest_fee:>16}   (estimated: a slot attestation measured {attest_gas} gas, {attest_bytes} B)")
print()
print(f"base fees at the endpoint now: execution {fee_state[0]}, storage {fee_state[1]}")
need_payer = total + attest_fee + 2 * reserve
need_daemon = 2 * per_day
print(f"payer {acc['payer']} (public key {acc['payer_public_key']}): at most {total + attest_fee} in fees if every transaction "
      f"used its whole gas limit; the sequencer holds each transaction's maximum fee (at most {reserve}) while it runs: "
      f"fund it with at least {need_payer}")
print(f"daemon key {acc['daemon']} (public key {acc['daemon_public_key']}): about {per_update} per finality update at these "
      f"base fees, at most {per_day} a day: fund it with at least {need_daemon} for two days")
print()
print("How to fund them on a LEZ v0.4 network (lez/testnet04/NOTES.md, Funds): native tokens reach a zone only through its")
print("bridge's Deposit: at genesis (the network's genesis actions) or from Logos Blockchain: a Channel Deposit of wallet")
print("notes to the zone's channel with the account id as the deposit's metadata (logos-docs, 'Bridge assets from Logos")
print("Blockchain to a Zone'; credited about an hour after the deposit's block). Send nothing before reading discover.json.")
json.dump({"rows": rows, "fee_state": fee_state, "need_payer": need_payer, "need_daemon": need_daemon,
           "payer": acc["payer"], "daemon": acc["daemon"]}, open(os.path.join(P, "plan.json"), "w"), indent=1)
EOF
  say "plan written: $STATE/plan.txt (nothing was sent)"
}

# ---- the daemon ---------------------------------------------------------------------------------------------------
DAEMON_DIR="$STATE/daemon"
daemon_pid() { [ -f "$DAEMON_DIR/pid" ] && kill -0 "$(cat "$DAEMON_DIR/pid")" 2>/dev/null && cat "$DAEMON_DIR/pid"; }
daemon_config() { # the updater's configuration: the instance day1 initialized (same preset, layout, adoption parts)
  local program checkpoint
  program="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["program"])' "$KEYS/instance.json")"
  checkpoint="$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["checkpoint"])' "$KEYS/instance.json")"
  mkdir -p "$DAEMON_DIR"
  cat > "$DAEMON_DIR/updater.toml" <<EOF
# Written by lez/testnet04/day1.sh: the day-one instance, followed on the public job path.
[daemon]
name = "day1"
state_dir = "state"
poll_secs = 12
listen = "127.0.0.1:$DAEMON_PORT"
ledger = "sequencer"

[sequencer]
rpc = "$URL"
program = "$program"
wait_final = true

[keys]
payer_key_file = "$KEYS/daemon.key"

[fees]
gas_limit = 4800000
max_fee_per_tx = ${FERRATA_DAY1_DAEMON_MAX_FEE:-1000000000}
budget = ${FERRATA_DAY1_DAEMON_BUDGET:-1000000000000}

[policy]                            # every field (the section has no per-field defaults); as the example's, but two sources
defer_window = 8
max_lag_slots = 64
min_agreeing = 2
retry_base_ms = 500
retry_max_ms = 30000
max_attempts = 5
demote_rounds = 50
max_periods_per_request = 16
finish_own_jobs = true

[[chain]]
name = "sepolia"
preset = "sepolia"
checkpoint = "$checkpoint"
layout = "gloas"
adoption_parts = 2
  [[chain.source]]
  name = "lodestar"
  url = "https://lodestar-sepolia.chainsafe.io"
  timeout_secs = 30
  kind = "light-client"
  [[chain.source]]
  name = "publicnode"
  url = "https://ethereum-sepolia-beacon-api.publicnode.com"
  kind = "headers"
EOF
}
daemon_start() {
  if pid="$(daemon_pid)"; then say "the daemon runs already (pid $pid)"; return 0; fi
  daemon_config
  say "starting the updater daemon (detached): $DAEMON_DIR, /health on 127.0.0.1:$DAEMON_PORT"
  # setsid: its own session, so that it outlives this script (and the command that started it); the pid is its own
  ( cd "$DAEMON_DIR" && FERRATA_TESTNET_SEND=yes setsid bash -c 'echo $$ > pid; exec "$0" --config updater.toml' "$UPDATER" \
      >> daemon.log 2>&1 < /dev/null & )
  sleep 2
  daemon_pid > /dev/null || { tail -n 20 "$DAEMON_DIR/daemon.log"; die "the daemon exited at once"; }
}
daemon_status() {
  if pid="$(daemon_pid)"; then echo "daemon: running, pid $pid"; else echo "daemon: not running"; fi
  curl -sS -m 5 "http://127.0.0.1:$DAEMON_PORT/health" 2>/dev/null || echo "(no /health answer)"
  echo
}
daemon_stop() {
  local pid
  pid="$(daemon_pid)" || { echo "daemon: not running"; return 0; }
  local wait="${FERRATA_DAY1_STOP_SECONDS:-120}"
  kill -TERM "$pid"
  for _ in $(seq 1 "$wait"); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
  if kill -0 "$pid" 2>/dev/null; then
    kill -KILL "$pid"
    echo "daemon: killed (it did not stop in $wait s): a transaction it was settling may have no journal row; the chain holds it"
  else
    echo "daemon: stopped"
  fi
}

# ---- run ----------------------------------------------------------------------------------------------------------
inspect_json() { "$TOOL" inspect --keys "$KEYS" --state "$STATE" $(bins) --rpc "$URL"; }
jq_py() { python3 -I -c "import json,sys; d=json.load(sys.stdin); print($1)"; }
stage_send() { # stage_send STAGE SIGN-ARGS...: sign once (kept), then send the stored bytes until every one settled
  local stage="$1"; shift
  if [ ! -d "$KEYS/tx/$stage" ]; then tool sign --keys "$KEYS" --state "$STATE" --stage "$stage" "$@"; fi
  tool send --keys "$KEYS" --state "$STATE" --stage "$stage"
}
instance_on_chain() { # the instance KEYS/instance.json names exists on chain (as of the last final block)
  [ -f "$KEYS/instance.json" ] && [ "$(inspect_json | jq_py 'd.get("instance",{}).get("exists")')" = True ]
}
stage_init() { # (b): nothing when the instance exists on chain; else sign once (from the checkpoint kept in KEYS/init/,
  # fetched the first time) and send the stored bytes. The tool refuses to sign when the instance exists (and writes
  # KEYS/instance.json): that is done too.
  if instance_on_chain; then say "(b) the instance $(jq_py 'd["instance_id"]' < "$KEYS/instance.json") exists on chain: no Init"; return 0; fi
  [ -f "$KEYS/init/chain.json" ] || fetch_checkpoint "$FETCH"
  if [ ! -d "$KEYS/tx/init" ]; then
    tool sign --keys "$KEYS" --state "$STATE" --stage init --fetch "$FETCH" \
      || { instance_on_chain && { say "(b) the instance exists on chain: no Init"; return 0; }; return 1; }
  fi
  tool send --keys "$KEYS" --state "$STATE" --stage init
}
run() {
  [ "${FERRATA_TESTNET_SEND:-}" = yes ] || die "refusing to send to $URL: set FERRATA_TESTNET_SEND=yes (the owner's decision)"
  need_tools; check_programs
  ensure_keys
  discover || die "discover: not a v0.4 network (or not answering): nothing sent"
  [ "$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["is_v04"])' "$STATE/discover.json")" = True ] \
    || die "discover: $URL is not a v0.4 network: nothing sent"
  local t0; t0=$(date +%s)
  {
    echo "run started $(date -u +%FT%TZ) against $URL"
  } >> "$STATE/run-log.txt"
  local before; before="$(inspect_json)"
  local bal; bal="$(echo "$before" | jq_py 'd["payer"]["balance"]')"
  [ "$bal" != 0 ] || die "the payer $(echo "$before" | jq_py 'd["payer"]["account"]') holds nothing: fund it first (day1.sh plan says how much)"

  # (a) the attestation program, byte for byte
  if [ "$(echo "$before" | jq_py 'd["program"]["binary"]["on_chain_equal"]')" = True ]; then
    say "(a) the program is deployed already, byte for byte"
  else
    say "(a) deploying the attestation program (0x${PROGRAM_IMAGE:0:8}…)"
    retry "(a) deploy" stage_send deploy-program --bin "$PROGRAMS/ferrata_lez.bin"
  fi
  until_stage a
  # (b) Init from a fresh finalized checkpoint, kept beside the keys at the first signing (KEYS/init/); nothing when
  # the instance exists on chain
  say "(b) Init of the Sepolia instance"
  retry "(b) Init" stage_init
  until_stage b
  # (c) the daemon (Adopt parts of the initial committee, then every update), detached
  say "(c) the updater daemon"
  daemon_start
  until_stage c
  # (d) one attestation of the live vault's slots 0 and 1, under the newest record once the daemon has applied an
  # update after the activation (a record of a block Sepolia's execution APIs still serve proofs for)
  if [ ! -f "$KEYS/tx/attest/00.result.json" ]; then
    say "(d) waiting for the instance to be active and to apply an update after its activation"
    local applied=0
    for _ in $(seq 1 240); do
      applied="$(inspect_json 2>/dev/null | jq_py 'd.get("instance",{}).get("applied",0) if d.get("instance",{}).get("active") else 0' 2>/dev/null || echo 0)"
      [ "${applied:-0}" -ge 2 ] && break
      sleep 15
    done
    [ "${applied:-0}" -ge 2 ] || die "(d) the instance has not applied an update after 60 min: day1.sh daemon-status"
    if [ ! -d "$KEYS/tx/attest" ]; then
      local target slot hash
      target="$(tool target --keys "$KEYS" --state "$STATE")"
      slot="$(echo "$target" | jq_py 'd["slot"]')"; hash="$(echo "$target" | jq_py 'd["block_hash"]')"
      say "(d) the newest record: slot $slot, execution block $hash; eth_getProof of $VAULT's slots $VAULT_SLOTS there"
      python3 -I "$FETCH_PY" proof "$FETCH" "$slot" "$hash" "$VAULT" "$VAULT_SLOTS"
    fi
    retry "(d) attestation" stage_send attest --fetch "$FETCH"
    python3 -I -c 'import json,sys; s=json.load(open(sys.argv[1])); print(json.dumps(s["info"]["verdict"], indent=1))' "$KEYS/tx/attest/stage.json"
  else
    say "(d) the attestation is applied already"
  fi
  # (e) optional: the bridge program, and the owner-side Sepolia step prepared (not sent)
  if [ "${FERRATA_DAY1_BRIDGE:-0}" = 1 ]; then
    say "(e) the bridge program (0x${BRIDGE_IMAGE:0:8}…)"
    if [ "$(inspect_json | jq_py 'd["bridge"]["binary"]["on_chain_equal"]')" != True ]; then
      retry "(e) deploy bridge" stage_send deploy-bridge --bin "$PROGRAMS/ferrata_bridge.bin"
    fi
    bridge_owner_step
  fi
  echo "run finished $(date -u +%FT%TZ), $(( $(date +%s) - t0 )) s" >> "$STATE/run-log.txt"
  say "run done in $(( $(date +%s) - t0 )) s; the daemon follows Sepolia: day1.sh daemon-status $URL (stop: day1.sh daemon-stop $URL)"
}
bridge_owner_step() { # what the owner's Sepolia step needs, written down; nothing is sent to Sepolia from here
  local acc="$KEYS/accounts.json" out="$STATE/bridge-owner-step.md"
  python3 -I - "$acc" "$KEYS/instance.json" "$BRIDGE_IMAGE" "$PROGRAM_IMAGE" "$out" <<'EOF'
import json, sys
acc, inst, bridge_image, program_image, out = sys.argv[1:6]
a, i = json.load(open(acc)), json.load(open(inst))
open(out, "w").write(f"""# The owner-side Sepolia step for the bridge on this network (prepared by day1.sh; not sent)

The bridge program is deployed at `{a['bridge']['header']}` (image `0x{bridge_image}`), beside the attestation program
`{a['program']['header']}` (image `0x{program_image}`), whose Sepolia instance is `0x{i['instance_id']}` (checkpoint
`0x{i['checkpoint']}`, slot {i['checkpoint_slot']}).

A bridge configuration pairs the bridge with one vault: its `configId` commits to the bridge's image id, the
instance and the vault's address, and the vault pins it (docs/design/bridge.md, docs/sepolia.md). The vault is deployed
by the owner on Sepolia with the release's deployment script, in its own step:

    scripts/deploy-testnet.sh   # plan first (no --send); its LEZ side reads the program accounts above

then the configuration transaction is signed here against the deployed vault's address (`day1.sh` stage (e) stops at
this point: the vault does not exist yet). Nothing in this file has been sent anywhere.
""")
print(out)
EOF
  say "(e) the owner-side Sepolia step: $out (not sent)"
}

# ---- verify -------------------------------------------------------------------------------------------------------
verify() {
  need_tools
  [ -d "$KEYS" ] || die "no keys at $KEYS"
  "$TOOL" verify --keys "$KEYS" --state "$STATE" $(bins) --rpc "$URL" --out "$STATE/verify.json" ${FERRATA_DAY1_MIN_APPLIED:+--min-applied $FERRATA_DAY1_MIN_APPLIED}
  # The instance's finalized header against a beacon node's canonical header at that slot (a second provider)
  python3 -I - "$STATE/verify.json" "$UA" <<'EOF'
import hashlib, json, sys, urllib.request
v = json.load(open(sys.argv[1])); ua = sys.argv[2]
inst = v["inspect"].get("instance") or {}
if not inst.get("exists"):
    sys.exit(0)
ssz = bytes.fromhex(inst["finalized_header_ssz"])
def h(a, b): return hashlib.sha256(a + b).digest()
leaves = [ssz[0:8].ljust(32, b"\0"), ssz[8:16].ljust(32, b"\0"), ssz[16:48], ssz[48:80], ssz[80:112]] + [b"\0" * 32] * 3
while len(leaves) > 1:
    leaves = [h(leaves[i], leaves[i + 1]) for i in range(0, len(leaves), 2)]
root = "0x" + leaves[0].hex()
slot = int.from_bytes(ssz[0:8], "little")
req = urllib.request.Request(f"https://ethereum-sepolia-beacon-api.publicnode.com/eth/v1/beacon/headers/{slot}", headers={"User-Agent": ua, "Accept": "application/json"})
try:
    got = json.loads(urllib.request.urlopen(req, timeout=30).read())["data"]["root"]
except Exception as e:
    print(f"[not checked] the finalized header at slot {slot}: publicnode did not answer ({e})"); sys.exit(3)
ok = got == root
print(f"[{'ok' if ok else 'FAIL'}] the instance's finalized header (slot {slot}) is publicnode's canonical block {got}")
v["checks"].append({"check": "finalized header canonical at publicnode", "ok": ok, "detail": f"slot {slot} root {root}"})
json.dump(v, open(sys.argv[1], "w"), indent=1)
sys.exit(0 if ok else 1)
EOF
}

case "$cmd" in
  discover) discover ;;
  plan) plan ;;
  run) run ;;
  go) plan; run; verify ;;
  verify) verify ;;
  status)
    need_tools
    for s in deploy-program deploy-bridge init attest; do
      [ -d "$KEYS/tx/$s" ] && { echo "-- $s"; tool status --keys "$KEYS" --state "$STATE" --stage "$s"; }
    done
    daemon_status ;;
  daemon-status) daemon_status ;;
  daemon-stop) daemon_stop ;;
esac
