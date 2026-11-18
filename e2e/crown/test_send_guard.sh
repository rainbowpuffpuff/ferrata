#!/usr/bin/env bash
# e2e/crown/sepolia.sh never sends while a transaction of the deployer may be in flight, and never sends twice
# (e2e/NETWORK.md). A local anvil that mines only when told (--no-mining); forge is a stub on PATH that sends one
# plain transfer through the --rpc-url it is given (the network proxy), as Crown.s.sol's one transaction would, and
# exits as the scenario says (ok, crash right after the broadcast, or nothing to do). Nothing leaves this machine.
#
#   bash e2e/crown/test_send_guard.sh
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-$repo/../tmp}/send-guard.XXXXXX")"
port="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
R="http://127.0.0.1:$port"
anvil --port "$port" --chain-id 11155111 --no-mining > "$scratch/anvil.log" 2>&1 & A=$!
trap 'kill $A 2>/dev/null; rm -rf "$scratch"' EXIT
for _ in $(seq 1 50); do cast chain-id --rpc-url "$R" > /dev/null 2>&1 && break; sleep 0.2; done
export STUB="$scratch/stub" KEY=0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d
mkdir -p "$scratch/bin" "$STUB"
run="$repo/e2e/runs/netfault-send-guard"; rm -rf "$run"; mkdir -p "$run"
cat > "$scratch/bin/forge" << 'EOF2'
#!/usr/bin/env bash
echo "forge $*" >> "$STUB/forge.calls"
url=""; while [ $# -gt 0 ]; do [ "$1" = --rpc-url ] && url="$2"; shift; done
v=$((100 + ${url##*-})) # one value per stage (deposit-j: 100 + j): each stage's call is its own step
case "$(cat "$STUB/mode")" in
  noop) exit 0 ;;
  crash) cast send --async --private-key "$KEY" --rpc-url "$url" 0x000000000000000000000000000000000000dEaD --value "$v"; exit 1 ;;
  ok) cast send --private-key "$KEY" --rpc-url "$url" 0x000000000000000000000000000000000000dEaD --value "$v" --timeout 30 ;;
esac
EOF2
chmod +x "$scratch/bin/forge"
echo '{"chainId": 11155111, "vault": "0x00000000000000000000000000000000000000aa"}' > "$run/identity.json"
export PATH="$scratch/bin:$PATH" SEPOLIA_RPC_URL="$R" ETH_FROM=0x70997970C51812dc3A010C7d01b50e0d17dc79C8 \
  PUBNET_REQUEST_BUDGET_S=4 PUBNET_STAGE_BUDGET_S=12 PUBNET_RESOLVE_BUDGET_S=8 PUBNET_BACKOFF_BASE_S=1 \
  PUBNET_TIMEOUT_S=3 PUBNET_REPLACED_AFTER_S=2 TMPDIR="$scratch"
J="$run/net/journal.jsonl"
pass=0 fail=0
calls() { [ -f "$STUB/forge.calls" ] && wc -l < "$STUB/forge.calls" || echo 0; }
signed() { [ -f "$J" ] && grep -c '"event": "signed"' "$J" || echo 0; }
pool() { cast rpc txpool_status --rpc-url "$R" | jq -r '.pending' | xargs printf '%d\n'; }
check() { # name, stage index, mode, expected exit, forge runs (0, 1, or 1+: retried), journaled sends, pool, text
  local name="$1" j="$2" mode="$3" want_rc="$4" want_calls="$5" want_signed="$6" want_pool="$7" want_text="$8" rc out c0 d
  echo "$mode" > "$STUB/mode"
  c0="$(calls)"
  out="$(bash "$here/sepolia.sh" "$run" deposit "$j" 2>&1)"; rc=$?
  d=$(( $(calls) - c0 ))
  if [ "$rc" = "$want_rc" ] && { [ "$d" = "$want_calls" ] || { [ "$want_calls" = 1+ ] && [ "$d" -ge 1 ]; }; } \
      && [ "$(signed)" = "$want_signed" ] && [ "$(pool)" = "$want_pool" ] && grep -q -e "$want_text" <<< "$out"; then
    echo "PASS  $name (exit $rc, forge runs $d, journaled $(signed), pool $(pool))"; pass=$((pass + 1))
  else
    echo "FAIL  $name: exit $rc (want $want_rc), forge runs $d (want $want_calls), journaled $(signed)" \
      "(want $want_signed), pool $(pool) (want $want_pool)"
    echo "$out" | tail -n 6 | sed 's/^/      /'; fail=$((fail + 1))
  fi
}
mine() { cast rpc evm_mine --rpc-url "$R" > /dev/null; }

# 1. A transaction of the deployer pending (sent around the proxy): the nonces do not agree, nothing is sent.
cast send --async --private-key "$KEY" --rpc-url "$R" 0x000000000000000000000000000000000000dEaD --value 2 > /dev/null
check "a pending transaction of the deployer: nothing sent, a transient stop" 0 ok 75 1+ 0 1 "STOP (transient"
mine
# 2. The send leaves (journaled first), then the run dies before its receipt: its fate is unknown, a transient stop.
check "the run dies right after its broadcast: journaled, a transient stop" 0 crash 75 1 1 1 "fate is still unknown\|STOP (transient"
# 3. Started again while that transaction is still unmined: forge is not even called.
check "a run while that fate is unknown: forge not called, nothing sent" 0 ok 75 0 1 1 "fate is still unknown"
# 4. Mined: the stage goes on (Crown.s.sol would see the deposit made and send nothing) and records it.
mine
check "the fate known (mined): the stage is done per the journal (forge not run), the transaction recorded" 0 noop 0 0 1 0 "deposit"
rows="$(jq -c '.stages["deposit-0"] | length' "$run/manifest.json" 2>/dev/null)"
if [ "$rows" = 1 ]; then echo "PASS  the manifest has deposit-0's one transaction (from the journal)"; pass=$((pass + 1));
else echo "FAIL  the manifest has $rows rows for deposit-0"; fail=$((fail + 1)); fi
# 5. Another transaction takes the nonce of a journaled one (replaced around the proxy): a permanent stop until an
#    operator has looked (pubnet.py ack); then the stage goes on.
check "a journaled send left in flight" 1 crash 75 1 2 1 "STOP (transient"
n="$(cast nonce "$ETH_FROM" --rpc-url "$R")"
cast send --async --private-key "$KEY" --rpc-url "$R" --nonce "$n" --gas-price 50gwei --priority-gas-price 40gwei \
  0x000000000000000000000000000000000000bEEF --value 3 > /dev/null
mine
check "its nonce taken by another transaction: replaced, a permanent stop, nothing sent" 1 ok 1 0 2 0 "replaced"
h="$(jq -r 'select(.event == "signed") | .hash' "$J" | tail -n 1)"
python3 -I "$repo/scripts/pubnet.py" ack --journal "$J" --hash "$h" --why "test: replaced on purpose"
check "after the operator's ack: the stage goes on" 1 noop 0 1 2 0 "deposit"

rm -rf "$run"
echo "send guard: $pass passed, $fail failed"
[ "$fail" = 0 ]
