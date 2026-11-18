#!/usr/bin/env bash
# testnet.sh's send guard, against stubs only (client, sequencer, fetch.py, curl): no network, no keys.
# From review-lez-v03's tests/sendguard (18 Nov 2026), turned into assertions: every path that can send to a
# non-loopback sequencer refuses without FERRATA_TESTNET_SEND=yes, which the script never sets itself; a loopback
# `local` run goes ahead. Usage: tests/sendguard.sh [path to testnet.sh] (default: ../testnet.sh). Exit 0 if all pass.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$(cd "$HERE/.." && pwd)/testnet.sh"; SRC="${1:-$SRC}"
W="$HERE/work-sendguard"; rm -rf "$W"
L="$W/spike/wt/lez-v03"
mkdir -p "$L/target/release" "$L/target/lez/release" "$L/scripts" "$L/results" "$W/bin" \
  "$W/spike/upstream/lez-v0.3.1/lez/sequencer/service/configs/debug"
cp "$SRC" "$L/testnet.sh"
echo '{}' > "$W/spike/upstream/lez-v0.3.1/lez/sequencer/service/configs/debug/sequencer_config.json"
cat > "$L/target/release/ferrata-v03" <<'S'
#!/usr/bin/env bash
[ $# -eq 0 ] && { echo usage; exit 2; }
echo "client $* | FERRATA_TESTNET_SEND=${FERRATA_TESTNET_SEND:-unset}" >> "$SG_LOG"
case "$1" in
  new-wallet) mkdir -p "$LEE_WALLET_HOME_DIR" "$2"; echo '{}' > "$LEE_WALLET_HOME_DIR/storage.json"
              echo "{\"payer\":\"PAYER\",\"program\":\"PROG\",\"sequencer\":\"$3\"}" > "$2/state.json"; echo PAYER ;;
  queue) printf 'seq\n0\n' > "$2/signed.tsv" ;;
  record) echo '{}' > "$2/record.json" ;;
  status) printf 'h0\t5\tPending\tstub\n' ;;
esac
exit 0
S
printf '#!/bin/sh\nsleep 30\n' > "$L/target/lez/release/sequencer_service"
cat > "$L/scripts/fetch.py" <<'S'
import json, os, sys
cmd, d = sys.argv[1], sys.argv[2]
os.makedirs(d, exist_ok=True)
if cmd == "checkpoint":
    json.dump({"bootstrap_slot": 100}, open(os.path.join(d, "chain.json"), "w"))
elif cmd == "finality":
    u = os.path.join(d, "updates"); os.makedirs(u, exist_ok=True)
    open(os.path.join(u, f"{len(os.listdir(u)):03d}-finality-{int(sys.argv[3]) + 32}.ssz"), "wb").write(bytes(1448))
elif cmd == "proof":
    p = os.path.join(d, "proofs", sys.argv[3]); os.makedirs(p, exist_ok=True); open(os.path.join(p, "proof.json"), "w").write("{}")
S
cat > "$W/bin/curl" <<'S'
#!/bin/sh
echo "curl $*" >> "$SG_LOG"
echo '{"jsonrpc":"2.0","id":1,"result":1}'
S
chmod +x "$L/target/release/ferrata-v03" "$L/target/lez/release/sequencer_service" "$W/bin/curl"
fail=0
case_() { # NAME EXPECT(refuse|go) ENV... -- ARGS...
  local name="$1" expect="$2"; shift 2
  local envs=(); while [ "$1" != -- ]; do envs+=("$1"); shift; done; shift
  : > "$W/log.txt"; rm -rf "$W/spike/tmp"
  (cd "$L" && env -u FERRATA_TESTNET_SEND -u FERRATA_SEQUENCER PATH="$W/bin:$PATH" SG_LOG="$W/log.txt" FERRATA_WAIT=1 \
     "${envs[@]}" timeout 60 bash ./testnet.sh "$@" > "$W/out.txt" 2>&1); local rc=$?
  local sent; sent="$(grep -c -E '^client (queue|resend|new-wallet)' "$W/log.txt" || true)"
  local public; public="$(grep -c 'testnet.lez.logos.co\|example.invalid' "$W/log.txt" || true)"
  if [ "$expect" = refuse ]; then
    if [ $rc -ne 0 ] && [ "$sent" -eq 0 ] && [ "$public" -eq 0 ]; then echo "PASS $name: refused (exit $rc), nothing sent"
    else echo "FAIL $name: exit $rc, $sent client sends, $public requests to the remote URL"; fail=1; fi
  else
    if [ "$sent" -gt 0 ] && ! grep -q 'FERRATA_TESTNET_SEND=yes' "$W/log.txt"; then echo "PASS $name: went ahead, the variable never forced"
    else echo "FAIL $name: exit $rc, $sent client calls"; cat "$W/out.txt"; fail=1; fi
  fi
}
case_ "local with FERRATA_SEQUENCER=public" refuse FERRATA_SEQUENCER=https://testnet.lez.logos.co -- local
case_ "local with FERRATA_SEQUENCER=another host" refuse FERRATA_SEQUENCER=https://example.invalid:3040 -- local
case_ "run against the public testnet without the variable" refuse -- run
case_ "run against another host without the variable" refuse FERRATA_SEQUENCER=https://example.invalid -- run
case_ "resend against the public testnet without the variable" refuse -- resend
case_ "local against loopback" go -- local
rm -rf "$W"
exit $fail
