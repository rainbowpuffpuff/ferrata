# Sourced by box.sh (`box.sh e2e [NAME]`): the wallet end to end on a local LEZ v0.4.0-rc3 sequencer.
#  1. LEZ's standalone sequencer (2 s blocks), its RPC on loopback;
#  2. LEZ's wallet CLI holding LEZ's published local test account A (lez/testnet_initial_state: a key anyone can read,
#     funded at genesis on a local network only): the faucet stand-in;
#  3. test/serve.py: the bundle, /rpc and /faucet on one origin, as the test network serves them;
#  4. test/e2e_test.py in headless Chromium (Python Playwright), checking each step against the RPC directly.
# Records: test/results/<NAME>/ (account ids and hashes replaced by labels), screenshots in box.sh's scratch
# (WORK) under e2e-<NAME>/.
NAME="${1:-local-$(date -u +%Y-%m-%d)}"
OUT="$WALLET/test/results/$NAME"
SHOTS="$WORK/e2e-$NAME"
rm -rf "$OUT" "$SHOTS"; mkdir -p "$OUT" "$SHOTS"
PORT="${WALLET_PORT:-18650}"

start_sequencer "${WALLET_BLOCK_TIME:-2s}"

# LEZ's published local test keys, read from LEZ's own source (never copied into this repository).
key_of() {
  python3 -I - "$LEZ_SRC/lez/testnet_initial_state/src/lib.rs" "$1" <<'EOF'
import re, sys
src, name = open(sys.argv[1]).read(), sys.argv[2]
m = re.search(r"const " + name + r": \[u8; 32\] = \[([^\]]*)\]", src)
print(bytes(int(x) for x in m.group(1).replace("\n", " ").split(",") if x.strip()).hex())
EOF
}
KEY_A="$(key_of PRIVATE_KEY_PUB_ACC_A)"; KEY_B="$(key_of PRIVATE_KEY_PUB_ACC_B)"
cli_wallet account list > "$RUN/logs/cli-init.log" 2>&1 || true
ID_A="$(cli_wallet account import public --private-key "$KEY_A" 2>&1 | grep -o -E 'Public/[1-9A-HJ-NP-Za-km-z]{32,44}' | head -1 | cut -d/ -f2)"
[ -n "$ID_A" ] || die "could not import LEZ's local test account A into the CLI wallet"
echo "== faucet stand-in: LEZ's local test account A ($(rpc getAccountBalance "[\"$ID_A\"]" | sed -n 's/.*"result":\([0-9]*\).*/\1/p') atomic units)"

python3 -I "$HERE/serve.py" --port "$PORT" --rpc "http://127.0.0.1:$RPC_PORT" --faucet-wallet "$BIN/wallet" \
  --faucet-from "$ID_A" --faucet-amount 5000000000 --faucet-interval 5 --faucet-log "$RUN/logs/faucet.json" \
  > "$RUN/logs/serve.log" 2>&1 &
SERVE_PID=$!
for _ in $(seq 1 50); do curl -fsS -m 2 "http://127.0.0.1:$PORT/" > /dev/null 2>&1 && break; sleep 0.2; done

set +e
WALLET_URL="http://127.0.0.1:$PORT/" WALLET_RPC="http://127.0.0.1:$RPC_PORT" WALLET_KEY_B="$KEY_B" \
  WALLET_FAUCET_ID="$ID_A" WALLET_OUT="$OUT" WALLET_SHOTS="$SHOTS" WALLET_ROOT="$WALLET" \
  python3 -I "$HERE/e2e_test.py" 2>&1 | tee "$RUN/logs/e2e.log"
rc=${PIPESTATUS[0]}
set -e
kill "$SERVE_PID" 2>/dev/null || true; sleep 0.5
cp "$RUN/logs/serve.log" "$SHOTS/" 2>/dev/null || true
cp "$RUN/logs/faucet.json" "$SHOTS/" 2>/dev/null || true
cp "$RUN/logs/sequencer.log" "$SHOTS/" 2>/dev/null || true
cat "$BIN/SHA256SUMS" "$BIN/toolchain.txt" > "$OUT/binaries.txt" 2>/dev/null || true
echo "== e2e exit $rc; record $OUT"
exit "$rc"
