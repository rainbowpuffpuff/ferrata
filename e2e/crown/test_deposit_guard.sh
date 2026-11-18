#!/usr/bin/env bash
# e2e/crown/sepolia.sh deposit never sends a deposit that may already be in flight. `cast` and `forge` are stubs on
# PATH (nothing leaves this machine): the stub forge records each call, so "nothing sent" means "forge not called".
#
#   bash e2e/crown/test_deposit_guard.sh
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-$repo/../tmp}/deposit-guard.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
export STUB="$scratch/stub"
mkdir -p "$scratch/bin" "$STUB" "$scratch/run"
cat > "$scratch/bin/cast" << 'EOF'
#!/usr/bin/env bash
case "$1" in
  chain-id) echo 11155111 ;;
  nonce) if [[ " $* " == *" --block pending "* ]]; then cat "$STUB/pending"; else cat "$STUB/latest"; fi ;;
  *) echo "stub cast: $*" >&2; exit 1 ;;
esac
EOF
cat > "$scratch/bin/forge" << 'EOF'
#!/usr/bin/env bash
echo "forge $*" >> "$STUB/forge.calls"
exit "$(cat "$STUB/forge.exit")"
EOF
chmod +x "$scratch/bin/cast" "$scratch/bin/forge"
echo '{"chainId": 11155111, "vault": "0x00000000000000000000000000000000000000aa"}' > "$scratch/run/identity.json"
export PATH="$scratch/bin:$PATH" SEPOLIA_RPC_URL=http://127.0.0.1:1 ETH_FROM=0x00000000000000000000000000000000000000bb \
  FERRATA_NONCE_GAP=0 FERRATA_RETRY_SLEEP=0

pass=0 fail=0
calls() { [ -f "$STUB/forge.calls" ] && wc -l < "$STUB/forge.calls" || echo 0; }
check() { # name, expected exit, expected forge calls, expected marker (yes/no), text the output must hold
  local name="$1" want_rc="$2" want_calls="$3" want_mark="$4" want_text="$5" rc out mark=no
  out="$(bash "$here/sepolia.sh" "$scratch/run" deposit 0 2>&1)"; rc=$?
  [ -f "$scratch/run/logs/deposit-0.sending" ] && mark=yes
  if [ "$rc" = "$want_rc" ] && [ "$(calls)" = "$want_calls" ] && [ "$mark" = "$want_mark" ] && grep -q -e "$want_text" <<< "$out"; then
    echo "PASS  $name"; pass=$((pass + 1))
  else
    echo "FAIL  $name: exit $rc (want $want_rc), forge calls $(calls) (want $want_calls), marker $mark (want $want_mark)"
    echo "$out" | tail -n 5 | sed 's/^/      /'; fail=$((fail + 1))
  fi
}

echo 5 > "$STUB/latest"; echo 6 > "$STUB/pending"; echo 0 > "$STUB/forge.exit"
check "a pending transaction of the depositor: nothing sent" 1 0 no "pending transaction"

echo 6 > "$STUB/latest"; echo 1 > "$STUB/forge.exit"
check "the send fails after it began (an interrupted run): the marker stays" 1 1 yes "STOP: deposit"

echo 0 > "$STUB/forge.exit"
check "a run after an interrupted send: nothing sent" 1 1 yes "a send began"

rm -f "$scratch/run/logs/deposit-0.sending"
check "the operator has checked the chain and removed the marker: one send, the marker cleared" 0 2 no "deposit"

echo "deposit guard: $pass passed, $fail failed"
[ "$fail" = 0 ]
