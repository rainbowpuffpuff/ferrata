#!/usr/bin/env bash
# Source verification of the round trip's four contracts on Sourcify (sourcify.dev; no account, no
# API key). Verification PUBLISHES the source files of a contract, for good, to a public service.
# Two groups, because they differ in what they publish:
#
#   script/sourcify.sh risc0     RiscZeroGroth16Verifier and RiscZeroVerifierEmergencyStop. Every file
#                                sent is public already: risc0-ethereum v3.0.0 and the OpenZeppelin
#                                v5.2.0 it pins (lib/risc0-ethereum/...).
#   script/sourcify.sh vault     FerrataVault and the test token. This publishes files of THIS
#                                repository:
#                                    src/FerrataVault.sol
#                                    src/interfaces/IRiscZeroVerifier.sol
#                                    script/TestToken.sol
#                                beside OpenZeppelin v5.4.0 files that are public already
#                                (IERC1363, IERC165, IERC20, SafeERC20, ReentrancyGuardTransient,
#                                TransientSlot; ERC20, IERC20Metadata, draft-IERC6093, Context).
#                                It runs only with FERRATA_PUBLISH_SOURCES=yes, to be set by whoever
#                                owns the repository.
#
# The addresses come from the fixture (FERRATA_FIXTURE, default ../e2e/runs/sepolia-live/fixture.json);
# the chain from its chain id. Each contract is sent with the compiler metadata of this build
# (solc 0.8.26, 200 runs, cancun), which names sources by their paths in this project.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export TMPDIR="${TMPDIR:-$here/../../tmp}" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
fixture="${FERRATA_FIXTURE:-$here/../e2e/runs/sepolia-live/fixture.json}"
chain="$(jq -r .chainId "$fixture")"
verify() { forge verify-contract --verifier sourcify --chain "$chain" "$(jq -r "$1" "$fixture")" "$2"; }
case "${1:?risc0 | vault}" in
risc0)
  verify .groth16Verifier lib/risc0-ethereum/contracts/src/groth16/RiscZeroGroth16Verifier.sol:RiscZeroGroth16Verifier
  verify .emergencyStop lib/risc0-ethereum/contracts/src/RiscZeroVerifierEmergencyStop.sol:RiscZeroVerifierEmergencyStop
  ;;
vault)
  if [ "${FERRATA_PUBLISH_SOURCES:-}" != yes ]; then
    echo "This publishes src/FerrataVault.sol, src/interfaces/IRiscZeroVerifier.sol and script/TestToken.sol on" >&2
    echo "sourcify.dev for good. Set FERRATA_PUBLISH_SOURCES=yes to do it." >&2
    exit 1
  fi
  verify .vault src/FerrataVault.sol:FerrataVault
  verify .token script/TestToken.sol:TestToken
  ;;
*) echo "risc0 | vault" >&2; exit 2 ;;
esac
