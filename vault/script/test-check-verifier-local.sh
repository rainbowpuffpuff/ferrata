#!/usr/bin/env bash
# script/check-verifier.sh against the wirings of script/CheckVerifierFixture.s.sol on a local anvil (nothing leaves the
# machine): the vault behind RISC Zero's Groth16 verifier 3.1.0 and its emergency stop with the owner renounced passes;
# a fake verifier that answers SELECTOR() and VERSION() like 3.1.0's, a fake stop that answers like RISC Zero's, a stop
# that keeps its owner and a tripped stop each fail, for their own reason (internal review of 27 November 2026, S-3;
# before it all five passed: only SELECTOR() was read).
#
#   script/test-check-verifier-local.sh        needs anvil, forge, cast, python3
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
port="$(python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
rpc="http://127.0.0.1:$port"
anvil --port "$port" --silent > /dev/null 2>&1 &
anvil_pid=$!
trap 'kill "$anvil_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 60); do cast chain-id --rpc-url "$rpc" > /dev/null 2>&1 && break; sleep 0.5; done
# anvil's first account, unlocked on anvil (the fixture refuses any other chain).
sender="$(cast rpc eth_accounts --rpc-url "$rpc" | python3 -I -c 'import json, sys; print(json.load(sys.stdin)[0])')"
out="$(forge script script/CheckVerifierFixture.s.sol:CheckVerifierFixture --rpc-url "$rpc" --unlocked --sender "$sender" --broadcast --slow 2>&1)" ||
  { echo "$out" | tail -20; echo "test-check-verifier-local: the fixture did not deploy"; exit 1; }
vault() { echo "$out" | awk -v n="$1" '$1 == "CHECKVERIFIER" && $2 == n { print $3 }'; }

expect() { # NAME ok|FAIL SUBSTRING
  local v r
  v="$(vault "$1")"
  [ -n "$v" ] || { echo "test-check-verifier-local: no $1 vault in the fixture's output"; exit 1; }
  if r="$(script/check-verifier.sh "$v" "$rpc" 2>&1)"; then got=ok; else got=FAIL; fi
  if [ "$got" != "$2" ] || [[ "$r" != *"$3"* ]]; then
    echo "test-check-verifier-local: $1: expected $2 with '$3', got $got: $r"; exit 1
  fi
  echo "  $1: $r"
}
expect ok ok "RISC Zero 3.1.0 (selector 0xe71e6620, code as pinned)"
expect fake-verifier FAIL "selector 0xe71e6620 but not RISC Zero 3.1.0's code"
expect fake-stop FAIL "not RISC Zero's emergency stop code"
expect owned-stop FAIL "it has an owner"
expect tripped-stop FAIL "tripped"
echo "TEST CHECK-VERIFIER LOCAL OK: the honest wiring passes; a fake verifier, a fake stop, an owned stop and a tripped stop fail"
