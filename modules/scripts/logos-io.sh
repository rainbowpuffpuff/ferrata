#!/usr/bin/env bash
# Our modules beside Logos Core's own, in one headless Logos Core (logosctl), the Ethereum reads going
# through Logos Core's eth_rpc_module (modules/LOGOS-CORE-MODULES.md):
#
#   1. Logos Core's modules built from their sources, each with logos-module-builder overridden to the
#      pin our modules use (5f74cdb): eth_rpc_module, beacon_api_module, and the send stack at the
#      revisions tx_sender_module's lock pins (keystore, signer_manager, fee, tx_sender).
#   2. check.sh for ferrata_attest_module and ferrata_bridge_module (dev packages, logosctl, their calls).
#   3. One Logos Core: all of them installed and loaded; then
#      e2e.sh        CALL=logosctl ETH_VIA=eth_rpc_module: fetch_log_proof (block, receipts) and the
#                    storage proof (dry run, Attest message) read through eth_rpc_module (chain 1 = the
#                    recorded mainnet block's server), each compared with the plain-HTTP read, then
#                    attested end to end in the LEZ harness;
#      bridge_e2e.sh CALL=logosctl ETH_VIA=eth_rpc_module: import_root reads the vault's eth_getProof
#                    through eth_rpc_module (chain 1 = the devnet's anvil), compared with the proofs
#                    given and the plain read, and the message submitted;
#      the send stack against the devnet's anvil: tx_sender_module.prepare (fee_module, eth_rpc_module).
#   4. summary.md: which call went through which module, the comparisons, the failures.
#
#   RELEASE_ELF=... modules/scripts/logos-io.sh OUT_DIR
# Needs what check.sh and bridge_e2e.sh need (Linux x86-64, Nix with flakes, cargo, Foundry, r0vm), and
# network access to GitHub for the Logos modules' sources. LOGOS_TMPDIR: as check.sh (62 bytes at most).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
: "${RELEASE_ELF:?RELEASE_ELF: the release guest binary (bridge/, cargo build --release -p methods)}"
BUILDER=github:logos-co/logos-module-builder/5f74cdb3cecacde054c68d72fcc0d2cfe2195900
export GIT_TERMINAL_PROMPT=0 FOUNDRY_DISABLE_NIGHTLY_WARNING=1 PATH="$HOME/.foundry/bin:$PATH"
if [ "$(ulimit -s)" = unlimited ]; then ulimit -s 1048576; fi
LOGOS_TMPDIR=${LOGOS_TMPDIR:-${TMPDIR:-/tmp}}
mkdir -p "$LOGOS_TMPDIR"
rep="$out/report"
mkdir -p "$rep/logs"
printf '| step | result | seconds |\n|---|---|---:|\n' > "$rep/steps.md"
failures=0
step() {
  local name=$1 t0 rc
  shift
  t0=$(date +%s)
  printf '%s %s\n' "$(date -u +%H:%M:%S)" "$name"
  if "$@" > "$rep/logs/$name.log" 2>&1; then rc=ok; else rc="FAILED (exit $?)"; failures=$((failures + 1)); fi
  printf '| %s | %s | %s |\n' "$name" "$rc" "$(( $(date +%s) - t0 ))" >> "$rep/steps.md"
  [ "$rc" = ok ]
}
# nix build, one at a time (two nix processes on the Modal box race on /homeless-shelter), retried on
# that box's two known failures (check.sh's nixb) and on a transient download error.
nixb() {
  local attempt log="$out/nix-build-attempt.log"
  for attempt in $(seq 1 40); do
    if [ "$(id -u)" = 0 ] && [ -d /homeless-shelter ]; then rm -rf /homeless-shelter; fi
    if nix build "$@" > >(tee "$log") 2>&1; then return 0; fi
    sleep 2
    grep -q -E 'reading a line: Input/output error|unexpected EOF reading a line|/homeless-shelter. exists|unable to download|HTTP error|timed out' "$log" || return 1
    echo "logos-io.sh: nix build $* stopped on a box or network error (attempt $attempt); retrying"
  done
  return 1
}

# 1. Logos Core's own modules (name, repository under github.com/logos-co, revision). eth_rpc_module is
# built from its source with its two optional dependencies called by name (eth_rpc_by_name.py: with
# our builder its install fails on their contracts); the others as they are.
cat > "$rep/logos-modules.tsv" <<'M'
eth_rpc_module	logos-evm-eth-rpc-module	6965f3252986eef02e9342198a0625fa725aad8d
beacon_api_module	logos-evm-beacon-api-module	aca138e5c211a370654700f4f8f2779ea5081c86
keystore_module	logos-evm-keystore-module	e83a899a1718ed52e3173dc85d06b15b9c68f41a
signer_manager_module	logos-signer-manager-module	bd5c84d21ced71246c4bf44d137aa1c597496fb0
fee_module	logos-evm-fee-module	f2065e787eebcd7fa49f6cf6ab1dc39457cba981
tx_sender_module	logos-evm-tx-sender-module	f1754c3b13c7c580ee449116ba8982ca193e114d
M
ethsrc="$out/src-eth_rpc_module"
rm -rf "$ethsrc"
step src-eth_rpc_module bash -c "git clone -q https://github.com/logos-co/logos-evm-eth-rpc-module.git '$ethsrc' \
  && git -C '$ethsrc' checkout -q 6965f3252986eef02e9342198a0625fa725aad8d \
  && python3 -I '$here/eth_rpc_by_name.py' '$ethsrc' && git -C '$ethsrc' add -A && git -C '$ethsrc' diff --cached --stat" || true
git -C "$ethsrc" diff --cached > "$rep/eth_rpc_module-by-name.diff" 2>/dev/null || true
while IFS=$'\t' read -r name repository rev; do
  if [ "$name" = eth_rpc_module ]; then flake="git+file://$ethsrc"; else flake="github:logos-co/$repository/$rev"; fi
  step "build-$name" nixb "$flake#lgx" --override-input logos-module-builder "$BUILDER" -o "$out/lgx-$name" || true
done < "$rep/logos-modules.tsv"

# 2. Our two modules: check.sh (its sdk step writes logos-rust-sdk-src as a nix result link: a copy
# left there by a native build is moved out of its way).
for m in ferrata_attest_module ferrata_bridge_module; do
  if [ -d "$repo/modules/$m/logos-rust-sdk-src" ] && [ ! -L "$repo/modules/$m/logos-rust-sdk-src" ]; then
    rm -rf "$repo/modules/$m/logos-rust-sdk-src"
  fi
done
# check.sh (stage, native tests and calls, dev packages, logosctl, calls in Logos Core).
step check env MODULES="ferrata_attest_module ferrata_bridge_module" PORTABLE=0 E2E=0 BRIDGE_E2E=0 \
  LOGOS_TMPDIR="$LOGOS_TMPDIR" "$here/check.sh" "$out/check" || true
cp "$out/check/report/summary.md" "$rep/check-summary.md" 2>/dev/null || true
LOGOSCTL="$out/check/logos/bin/logosctl"

# 3. One Logos Core with Logos Core's modules and ours.
export TMPDIR="$LOGOS_TMPDIR"
SESSION="$LOGOS_TMPDIR/lio-session"
rm -rf "$SESSION" "$out/home"
mkdir -p "$out/home"
ctl() { "$LOGOSCTL" --config-dir "$SESSION" "$@"; }
trap 'ctl daemon stop > /dev/null 2>&1 || true' EXIT
step daemon-start env HOME="$out/home" RISC0_DEV_MODE=1 "$LOGOSCTL" --config-dir "$SESSION" daemon start --detach
while IFS=$'\t' read -r name repository rev; do
  if ls "$out/lgx-$name"/*.lgx > /dev/null 2>&1; then step "install-$name" ctl install "$out/lgx-$name"/*.lgx -y || true; fi
done < "$rep/logos-modules.tsv"
for m in ferrata_attest_module ferrata_bridge_module; do step "install-$m" ctl install "$out/check/lgx-$m"/*.lgx -y || true; done
# eth_rpc_module first; our modules name it an optional dependency (loaded with them when installed).
for m in eth_rpc_module ferrata_attest_module ferrata_bridge_module beacon_api_module tx_sender_module; do
  step "load-$m" ctl module load "$m" || true
done
ctl module list --json > "$rep/modules-loaded.json" 2>&1 || true
for m in ferrata_attest_module ferrata_bridge_module eth_rpc_module tx_sender_module; do
  ctl module show "$m" --json > "$rep/show-$m.json" 2>&1 || true
done

step e2e env CALL=logosctl ETH_VIA=eth_rpc_module LOGOSCTL="$LOGOSCTL" SESSION="$SESSION" \
  RISC0_HOME="${RISC0_HOME:-$repo/../risc0-home-lez}" "$here/e2e.sh" "$out/e2e" || true
step bridge-e2e env CALL=logosctl ETH_VIA=eth_rpc_module LOGOSCTL="$LOGOSCTL" SESSION="$SESSION" \
  RELEASE_ELF="$RELEASE_ELF" "$here/bridge_e2e.sh" "$out/bridge-e2e" || true

# The send stack against a local anvil, driven as an operator would from the command line: a transfer
# priced by tx_sender_module (fee_module and eth_rpc_module read the chain), then sent: anvil's first
# dev key imported into keystore_module, the roles given to this command line (the caller identity the
# modules report), an unattended unlock scoped to tx_sender_module (signer_manager_module's `unlock`
# with confirm false), tx_sender_module.send, send_status polled until final, the receipt read back
# through eth_rpc_module. Informational: each reply is recorded in send-stack.log; it is not counted
# as a failure of this run.
anvil --port 18545 --chain-id 31337 --silent > "$out/anvil.log" 2>&1 &
anvil=$!
for _ in $(seq 1 50); do (echo > /dev/tcp/127.0.0.1/18545) 2>/dev/null && break; sleep 0.2; done
DEV0=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
DEV1=0x70997970C51812dc3A010C7d01b50e0d17dc79C8
DEV0_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80  # anvil's public dev key 0
sl="$rep/send-stack.log"
c() { local label=$1; shift; printf '\n## %s\n' "$label" >> "$sl"; ctl call "$@" --json < /dev/null >> "$sl" 2>&1; }
r() { ctl call "$@" --json < /dev/null 2>/dev/null | jq -r '.result // empty' 2>/dev/null; }
: > "$sl"
c "eth_rpc_module chain 31337 = anvil" eth_rpc_module set_chain_config 31337 '{"endpoint":"http://127.0.0.1:18545","name":"anvil","testnet":true}'
c "eth_rpc_module block_number" eth_rpc_module block_number 31337
c "tx_sender_module prepare" tx_sender_module prepare "{\"chainId\":31337,\"from\":\"$DEV0\",\"calls\":[{\"to\":\"$DEV1\",\"value\":\"1000\"}]}"
c "keystore_module caller_identity" keystore_module caller_identity
me=$(r keystore_module caller_identity | jq -r '.identity // empty' 2>/dev/null)
echo "caller identity: ${me:-unknown}" >> "$sl"
if [ -n "$me" ]; then
  c "keystore_module configure" keystore_module configure "{\"approvers\":[\"$me\"],\"custodians\":[\"$me\"],\"managers\":[\"signer_manager_module\"]}"
  c "keystore_module import_private_key" keystore_module import_private_key "$DEV0_KEY" "ferrata-devnet-password"
  c "signer_manager_module configure" signer_manager_module configure "{\"approvers\":[\"$me\"],\"custodians\":[\"$me\"],\"signers\":[\"keystore_module\"]}"
  c "signer_manager_module accounts" signer_manager_module accounts
  c "signer_manager_module unlock" signer_manager_module unlock "{\"account\":\"$DEV0\",\"password\":\"ferrata-devnet-password\",\"ttlMs\":600000,\"count\":5,\"apps\":[\"tx_sender_module\"],\"confirm\":false}"
  sr=$(r tx_sender_module send "{\"chainId\":31337,\"from\":\"$DEV0\",\"calls\":[{\"to\":\"$DEV1\",\"value\":\"1000\"}],\"purpose\":\"Ferrata logos-io: a devnet transfer\"}")
  printf '\n## tx_sender_module send\n%s\n' "$sr" >> "$sl"
  rid=$(echo "$sr" | jq -r '.requestId // empty' 2>/dev/null)
  echo "requestId: ${rid:-none}" >> "$sl"
  if [ -n "$rid" ]; then
    for i in $(seq 1 15); do
      st=$(r tx_sender_module send_status "$rid")
      printf '\n## send_status %s\n%s\n' "$i" "$st" >> "$sl"
      [ "$(echo "$st" | jq -r '.final // false' 2>/dev/null)" = true ] && break
      sleep 2
    done
    hash=$(echo "$st" | jq -r '.hash // empty' 2>/dev/null)
    [ -n "$hash" ] && c "eth_rpc_module get_transaction_receipt" eth_rpc_module get_transaction_receipt 31337 "$hash"
  fi
  c "signer_manager_module unattended_signatures" signer_manager_module unattended_signatures
fi
kill $anvil 2>/dev/null || true
cp "$SESSION/logs/daemon.log" "$rep/daemon.log" 2>/dev/null || true

# 4. Summary.
python3 -I "$here/logos_io_summary.py" "$out" "$failures"
