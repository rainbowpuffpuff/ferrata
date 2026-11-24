#!/usr/bin/env bash
# CI's check of e2e/seqnet's vault deployment (job `vault`): the functions drive.sh runs for every mode (vault.sh), on a
# local anvil (chain id 11155111, as drive.sh starts it), with a stand-in configuration id and genesis state: DeployVault
# through e2e/crown/sepolia.sh deploy (it deploys RISC Zero's 3.1.0 verifier itself and checks the vault against the
# run's identity), then the mock's code in the verifier's place. It fails as drive.sh would: on 23 Nov 2026 DeployVault
# began refusing the mock drive.sh passed in, and every seqnet mode but `deployer` stopped at "vault deployment" with no
# CI job noticing (review of priv7-gasdrop, F3).
#
#   e2e/seqnet/test-vault-deploy.sh      after `forge build` (or `forge test`) in vault/ with its libraries
#
# Local only: anvil on a free loopback port (scripts/pubnet.py makes a local endpoint the only one); records in
# e2e/runs/ci-seqnet-vault/ (removed at the end; forge reads params.json there), nothing kept.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
. "$REPO/e2e/seqnet/vault.sh"
export PATH="$HOME/.foundry/bin:$PATH" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
RUN="$REPO/e2e/runs/ci-seqnet-vault"; W="$(mktemp -d "${TMPDIR:-$REPO/../tmp}/seqnet-vault.XXXXXX" 2>/dev/null || mktemp -d)"
rm -rf "$RUN"; mkdir -p "$RUN"
PORT="$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
RPC="http://127.0.0.1:$PORT"
DEPLOYER=0x000000000000000000000000000000005E9DE901   # drive.sh's: an address nobody holds a key for, impersonated
anvil --chain-id 11155111 --port "$PORT" --gas-limit 100000000 --auto-impersonate > "$W/anvil.log" 2>&1 & ANVIL=$!
cleanup() { kill "$ANVIL" 2>/dev/null || true; rm -rf "$RUN" "$REPO/vault/deployments/crown-ci-seqnet-vault.json" "$W"; }
trap cleanup EXIT
for _ in $(seq 60); do cast chain-id --rpc-url "$RPC" > /dev/null 2>&1 && break; sleep 0.5; done
cast rpc anvil_setBalance "$DEPLOYER" 0x56bc75e2d63100000 --rpc-url "$RPC" > /dev/null
VAULT="$(seqnet_vault_address "$RPC" "$DEPLOYER")"
seqnet_vault_inputs "$RUN" "$VAULT" 0x$(printf '11%.0s' {1..32}) 0x$(printf '22%.0s' {1..32}) \
  "RISC Zero's Groth16 verifier deployed by DeployVault, its code then replaced on anvil by the vault tests' MockRiscZeroVerifier" \
  0x$(printf 'a1%.0s' {1..32}) 0x$(printf 'b1%.0s' {1..32}) 0x$(printf 'c1%.0s' {1..32}) 3600
if ! seqnet_deploy_vault "$RUN" "$RPC" "$DEPLOYER" "$W/deploy.log"; then
  tail -n 40 "$W/deploy.log"; echo "FAIL: e2e/seqnet's vault deployment (drive.sh, every mode) is refused" >&2; exit 1
fi
MOCK="$(seqnet_mock_verifier "$RUN" "$RPC")"
call() { cast call "$@" --rpc-url "$RPC"; }
[ "$(jq -r .vault "$RUN/deploy.json")" = "$VAULT" ] || { echo "FAIL: the vault is not at the address drive.sh computes ($VAULT)" >&2; exit 1; }
[ "$(call "$VAULT" 'mode()(uint8)')" = 1 ] || { echo "FAIL: the vault does not run rule (B)" >&2; exit 1; }
STOP="$(call "$VAULT" 'verifier()(address)')"
[ "$(call "$STOP" 'verifier()(address)' | tr A-F a-f)" = "$(echo "$MOCK" | tr A-F a-f)" ] || { echo "FAIL: the stop does not wrap the swapped verifier" >&2; exit 1; }
[ "$(call "$MOCK" 'accepted(bytes32,bytes32)(bool)' 0x$(printf '00%.0s' {1..32}) 0x$(printf '00%.0s' {1..32}))" = false ] \
  || { echo "FAIL: the mock's code is not at the verifier's address" >&2; exit 1; }
[ "$(jq -r .groth16Verifier "$RUN/identity.json" | tr A-F a-f)" = "$(echo "$MOCK" | tr A-F a-f)" ] || { echo "FAIL: identity.json" >&2; exit 1; }
echo "ok: e2e/seqnet's vault deployment: vault $VAULT (rule B), DeployVault's verifier $MOCK carries the mock's code behind the stop $STOP"
