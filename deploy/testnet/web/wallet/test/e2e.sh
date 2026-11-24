# Sourced by box.sh (`box.sh e2e [NAME]`): the wallet end to end on a local LEZ v0.4.0-rc3 sequencer.
#  1. LEZ's standalone sequencer (2 s blocks), its RPC on loopback;
#  2. LEZ's wallet CLI holding LEZ's published local test account A (lez/testnet_initial_state: a key anyone can read,
#     funded at genesis on a local network only): the faucet stand-in;
#  3. test/serve.py: the bundle, /rpc and /faucet on one origin, as the test network serves them;
#  4. test/e2e_test.py in headless Chromium (Python Playwright), checking each step against the RPC directly.
# Records: test/results/<NAME>/ (account ids and hashes replaced by labels), screenshots beside the repository in
# ferrata-scratch/wallet/e2e-<NAME>/.
NAME="${1:-local-$(date -u +%Y-%m-%d)}"
OUT="$WALLET/test/results/$NAME"
SHOTS="$BESIDE/ferrata-scratch/wallet/e2e-$NAME"
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

# The bridge's Ethereum side, when Foundry and the vault's libraries are here: anvil as "Sepolia" (chain id 11155111,
# loopback only) and FerrataVault with a registered ETH (vault/script/WebDepositorLocal.s.sol, as web/depositor's test).
CONFIG="$RUN/config.json"; cp "$WALLET/config.json" "$CONFIG"
ANVIL_URL=""; VAULT=""; DEPOSITOR=""
if command -v anvil > /dev/null && [ -d "$REPO/vault/lib/forge-std" ] && [ "${WALLET_BRIDGE:-1}" = 1 ]; then
  ANVIL_PORT=18645; ANVIL_URL="http://127.0.0.1:$ANVIL_PORT"
  anvil --host 127.0.0.1 --port "$ANVIL_PORT" --chain-id 11155111 --code-size-limit 100000 > "$RUN/logs/anvil.log" 2>&1 &
  ANVIL_PID=$!
  ejson() { curl -sS -m 5 -H 'Content-Type: application/json' --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":${2:-[]}}" "$ANVIL_URL"; }
  for _ in $(seq 1 100); do ejson eth_chainId 2>/dev/null | grep -q 0xaa36a7 && break; sleep 0.2; done
  ACCTS="$(ejson eth_accounts)"
  SENDER="$(printf '%s' "$ACCTS" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["result"][0])')"
  DEPOSITOR="$(printf '%s' "$ACCTS" | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["result"][1])')"
  ( cd "$REPO/vault" && forge build --offline > "$RUN/logs/forge-build.log" 2>&1 ) || { tail -20 "$RUN/logs/forge-build.log"; die "forge build failed"; }
  ( cd "$REPO/vault" && WEB_DEPOSITOR_ACCOUNT="$DEPOSITOR" forge script script/WebDepositorLocal.s.sol --rpc-url "$ANVIL_URL" \
      --broadcast --unlocked --sender "$SENDER" --offline --slow > "$RUN/logs/forge-deploy.log" 2>&1 ) || { tail -20 "$RUN/logs/forge-deploy.log"; die "vault deployment failed"; }
  VAULT="$(grep -Eo 'FERRATA_WEB_VAULT 0x[0-9a-fA-F]{40}' "$RUN/logs/forge-deploy.log" | head -1 | awk '{print $2}')"
  [ -n "$VAULT" ] || die "no vault address in the deployment log"
  python3 -I - "$CONFIG" "$VAULT" <<'PY'
import json, sys
p, vault = sys.argv[1:3]
c = json.load(open(p)); c["bridge"].update({"vault": vault, "evmChainId": 11155111}); c["network"] = "local LEZ rc3 + local Sepolia stand-in"
json.dump(c, open(p, "w"), indent=1)
PY
  echo "== bridge: anvil (chain id 11155111) and the vault deployed"
fi

python3 -I "$HERE/serve.py" --port "$PORT" --config "$CONFIG" --rpc-delay-ms "${WALLET_RPC_DELAY_MS:-250}" --rpc "http://127.0.0.1:$RPC_PORT" --faucet-wallet "$BIN/wallet" \
  --faucet-from "$ID_A" --faucet-amount 5000000000 --faucet-interval 5 --faucet-log "$RUN/logs/faucet.json" \
  > "$RUN/logs/serve.log" 2>&1 &
SERVE_PID=$!
for _ in $(seq 1 50); do curl -fsS -m 2 "http://127.0.0.1:$PORT/" > /dev/null 2>&1 && break; sleep 0.2; done

set +e
WALLET_ANVIL="$ANVIL_URL" WALLET_VAULT="$VAULT" WALLET_DEPOSITOR="$DEPOSITOR" \
WALLET_URL="http://127.0.0.1:$PORT/" WALLET_RPC="http://127.0.0.1:$RPC_PORT" WALLET_KEY_B="$KEY_B" \
  WALLET_FAUCET_ID="$ID_A" WALLET_OUT="$OUT" WALLET_SHOTS="$SHOTS" WALLET_ROOT="$WALLET" \
  python3 -I "$HERE/e2e_test.py" 2>&1 | tee "$RUN/logs/e2e.log"
rc=${PIPESTATUS[0]}
set -e
kill "$SERVE_PID" 2>/dev/null || true; sleep 0.5
cp "$RUN/logs/serve.log" "$SHOTS/" 2>/dev/null || true
cp "$RUN/logs/faucet.json" "$SHOTS/" 2>/dev/null || true
cp "$RUN/logs/sequencer.log" "$SHOTS/" 2>/dev/null || true
cp "$RUN/logs/anvil.log" "$RUN/logs/forge-deploy.log" "$SHOTS/" 2>/dev/null || true
cat "$BIN/SHA256SUMS" "$BIN/toolchain.txt" > "$OUT/binaries.txt" 2>/dev/null || true
echo "== e2e exit $rc; record $OUT"
exit "$rc"
