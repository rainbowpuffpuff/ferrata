#!/usr/bin/env bash
# Source verification of a round trip's contracts on Sourcify (sourcify.dev; no account, no
# API key). Verification PUBLISHES the source files of a contract, for good, to a public service.
# Two groups, because they differ in what they publish, and a read-only check:
#
#   script/sourcify.sh risc0     RiscZeroGroth16Verifier and RiscZeroVerifierEmergencyStop. Every file
#                                sent is public already: risc0-ethereum as lib.lock pins it (v3.1.0
#                                since 23 November 2026; a vault of November 2026 was deployed from
#                                v3.0.0, whose tree must be installed in lib/ to match it) and the
#                                OpenZeppelin v5.2.0 it pins (lib/risc0-ethereum/...).
#   script/sourcify.sh vault     FerrataRegistry (the library the vault is linked to), FerrataVault and
#                                the test token. This publishes files of THIS repository:
#                                    src/FerrataVault.sol
#                                    src/FerrataRegistry.sol
#                                    src/interfaces/IEmergencyStop.sol
#                                    src/interfaces/IRiscZeroVerifier.sol
#                                    script/TestToken.sol
#                                beside OpenZeppelin v5.4.0 files that are public already
#                                (IERC1363, IERC165, IERC20, SafeERC20, ReentrancyGuardTransient,
#                                TransientSlot; ERC20, IERC20Metadata, draft-IERC6093, Context).
#                                It runs only with FERRATA_PUBLISH_SOURCES=yes, to be set by whoever
#                                owns the repository.
#   script/sourcify.sh status    Sends nothing: prints what Sourcify holds for each address.
#
# The addresses come from the run's record (FERRATA_FIXTURE, default
# ../e2e/runs/crown-live-3/deploy.json); the chain from its chain id. Each contract is sent with the
# compiler metadata of this build (solc 0.8.26, 200 runs, cancun), which names sources by their
# paths in this project. The vault is sent unlinked, as it was compiled: forge wrote the registry's
# address into the bytecode at deployment, so the metadata names no library, and Sourcify reads the
# address from the deployed code. Passing --libraries would change the metadata hash and lose the
# exact match. Run it from the tree the contracts were deployed from: a changed comment changes
# the hash too.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
export TMPDIR="${TMPDIR:-$here/../../tmp}" FOUNDRY_DISABLE_NIGHTLY_WARNING=1
fixture="${FERRATA_FIXTURE:-$here/../e2e/runs/crown-live-3/deploy.json}"
chain="$(jq -r .chainId "$fixture")"
address() { jq -r "$1 // empty" "$fixture"; }
verify() { forge verify-contract --verifier sourcify --chain "$chain" "$(address "$1")" "$2"; }
case "${1:?risc0 | vault | status}" in
risc0)
  verify .groth16Verifier lib/risc0-ethereum/contracts/src/groth16/RiscZeroGroth16Verifier.sol:RiscZeroGroth16Verifier
  verify .emergencyStop lib/risc0-ethereum/contracts/src/RiscZeroVerifierEmergencyStop.sol:RiscZeroVerifierEmergencyStop
  ;;
vault)
  if [ "${FERRATA_PUBLISH_SOURCES:-}" != yes ]; then
    echo "This publishes src/FerrataVault.sol, src/FerrataRegistry.sol, src/interfaces/IEmergencyStop.sol," >&2
    echo "src/interfaces/IRiscZeroVerifier.sol and script/TestToken.sol on sourcify.dev for good." >&2
    echo "Set FERRATA_PUBLISH_SOURCES=yes to do it." >&2
    exit 1
  fi
  # The registry first: a vault before v4 has none, and its record has no such field.
  [ -z "$(address .registry)" ] || verify .registry src/FerrataRegistry.sol:FerrataRegistry
  verify .vault src/FerrataVault.sol:FerrataVault
  verify .token script/TestToken.sol:TestToken
  ;;
status)
  # "exact_match": this source and its metadata hash. "match": the same code under another
  # metadata hash, which Sourcify also grants by itself to a contract whose code equals one
  # somebody else verified, under that contract's name. "none": Sourcify holds nothing.
  for field in vault registry token emergencyStop groth16Verifier; do
    a="$(address ".$field")"; [ -n "$a" ] || continue
    reply="$(curl -s -m 30 -A curl "https://sourcify.dev/server/v2/contract/$chain/$a?fields=compilation" || true)"
    printf '%-16s %s  %s  %s\n' "$field" "$a" \
      "$(jq -r '.match // "none"' <<<"$reply" 2>/dev/null || echo unreadable)" \
      "$(jq -r '.compilation.fullyQualifiedName // ""' <<<"$reply" 2>/dev/null || true)"
  done
  ;;
*) echo "risc0 | vault | status" >&2; exit 2 ;;
esac
