# Sourced: the vault's deployment on a local anvil for e2e/seqnet (drive.sh, every mode) and for its CI check
# (e2e/seqnet/test-vault-deploy.sh, CI job `vault`), so that the two cannot drift apart.
#
# DeployVault accepts only RISC Zero's 3.1.0 Groth16 verifier (its VERSION(), SELECTOR() and exact code: a 3.0.x one
# accepts forged proofs), so it always deploys that verifier itself (the vault is then its fourth creation: verifier,
# stop, library, vault). Every mode but `deployer` replaces the verifier's code on anvil by the vault tests'
# MockRiscZeroVerifier afterwards, as scripts/ci/sequencer-roundtrip-hook.sh does; passing the mock in instead is refused
# by DeployVault since 23 Nov 2026, which stopped every such mode at "vault deployment" until this was shared.

# seqnet_vault_address RPC DEPLOYER: where DeployVault's vault will be (the deployer's nonce + 3).
seqnet_vault_address() {
  local n
  n="$(cast nonce "$2" --rpc-url "$1")" || return 1
  cast compute-address "$2" --nonce $((n + 3)) | awk '{print $NF}'
}

# seqnet_deploy_vault RUN RPC DEPLOYER LOG: e2e/crown/sepolia.sh RUN deploy, the deployer impersonated (RUN holds
# identity.json, params.json and crown.json, and must be under e2e/runs/).
seqnet_deploy_vault() {
  local here; here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  SEPOLIA_RPC_URL="$2" ETH_FROM="$3" FERRATA_UNLOCKED=1 FERRATA_NONCE_GAP=0 bash "$here/e2e/crown/sepolia.sh" "$1" deploy > "$4" 2>&1
}

# seqnet_mock_verifier RUN RPC: the Groth16 verifier DeployVault deployed (RUN/deploy.json), its code replaced by
# MockRiscZeroVerifier's (vault/out, forge build), RUN/identity.json's groth16Verifier set to it; prints its address.
seqnet_mock_verifier() {
  local here gv code
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  gv="$(jq -r '.groth16Verifier // empty' "$1/deploy.json")"
  [ -n "$gv" ] || { echo "seqnet_mock_verifier: $1/deploy.json names no Groth16 verifier" >&2; return 1; }
  code="$(jq -r '.deployedBytecode.object // empty' "$here/vault/out/MockVerifiers.sol/MockRiscZeroVerifier.json" 2>/dev/null)"
  [ -n "$code" ] && [ "$code" != 0x ] || { echo "seqnet_mock_verifier: no MockRiscZeroVerifier in vault/out (forge build)" >&2; return 1; }
  cast rpc anvil_setCode "$gv" "$code" --rpc-url "$2" > /dev/null || return 1
  [ "$(cast code "$gv" --rpc-url "$2")" = "$code" ] || { echo "seqnet_mock_verifier: the code did not take" >&2; return 1; }
  jq --arg g "$gv" '.groth16Verifier = $g' "$1/identity.json" > "$1/identity.json.tmp" && mv "$1/identity.json.tmp" "$1/identity.json"
  echo "$gv"
}

# seqnet_vault_inputs RUN VAULT CID GS VERIFIER_LABEL CHECKPOINT_A_ID CHECKPOINT_B_ID RELEASE_ID DELTA: DeployVault's
# inputs in RUN (identity.json, params.json, crown.json), the groth16Verifier left empty: DeployVault deploys it.
seqnet_vault_inputs() {
  local z=0x0000000000000000000000000000000000000000000000000000000000000000
  jq -n --arg v "$2" --arg c "$3" --arg g "$4" --arg d "$9" --arg w "$5" \
    '{label: ("seqnet: local anvil (chain id 11155111, the stand-in committee finalizes its blocks), the vault uses " + $w + " behind the emergency stop"),
    chainId: 11155111, vault: $v, configId: $c, genesisState: $g, groth16Verifier: "", delta: ($d | tonumber)}' > "$1/identity.json"
  jq -n --arg c "$3" --arg g "$4" --arg a "$6" --arg b "$7" --arg r "$8" --arg z "$z" \
    '{_source: "seqnet (the final programs of docs/image-ids.md; a stand-in identity from a real devnet session A)",
      checkpointImageIdA: $a, checkpointImageIdB: $b, releaseImageId: $r, lezConfigId: $c, genesisState: $g, nMin: 1, tMin: 1,
      postingTimeframe: 0, ethLezDefinition: $z, tokenLezDefinition: $z}' > "$1/params.json"
  jq -n --arg v "$2" '{chainId: 11155111, vault: $v, deposits: []}' > "$1/crown.json"
}
