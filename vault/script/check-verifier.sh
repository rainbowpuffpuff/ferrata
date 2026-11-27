#!/usr/bin/env bash
# The Groth16 verifier behind a vault's emergency stop, read from the chain (nothing is sent):
#   SELECTOR() 0xe71e6620  RISC Zero's verifier 3.1.0: ok only if, too, the verifier's code is RISC Zero's 3.1.0 as
#                          script/DeployVault.s.sol deploys it, the stop's code is RISC Zero's emergency stop (its one
#                          immutable, the wrapped verifier's address, masked), the stop has no owner (nobody can trip
#                          it at will) and it is not tripped: the code hashes of script/params/verifier-31.json, which
#                          test/VerifierCode.t.sol computes from vault/lib.lock's risc0-ethereum v3.1.0 (internal review
#                          of 27 November 2026, S-3: a contract answering SELECTOR() and VERSION() passed before);
#   SELECTOR() 0x73c457ba  verifier 3.0.0, which accepts forged claims (GHSA-mm6c-35cm-g4x2): ok only while the vault's
#                          deposit and release caps both read 0 (the four vaults of November 2026, capped by the owner
#                          on 21 November 2026: docs/security/risc0-groth16-advisory-2026-11-21); else FAIL;
#   anything else          FAIL.
#
#   script/check-verifier.sh VAULT RPC_URL        exit 0 (ok) or 1 (FAIL)
set -euo pipefail
vault="${1:?vault address}"; rpc="${2:?RPC URL}"
export FOUNDRY_DISABLE_NIGHTLY_WARNING=1
call() { cast call "$@" --rpc-url "$rpc"; }
stop="$(call "$vault" 'verifier()(address)')"
groth16="$(call "$stop" 'verifier()(address)')"
sel="$(call "$groth16" 'SELECTOR()(bytes4)')"
pin() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' \
  "$(dirname "$0")/params/verifier-31.json" "$1"; }
fail() { echo "FAIL  $*"; exit 1; }
case "$sel" in
  0xe71e6620)
    [ "$(cast codehash "$groth16" --rpc-url "$rpc")" = "$(pin groth16CodeHash)" ] ||
      fail "Groth16 verifier $groth16: selector 0xe71e6620 but not RISC Zero 3.1.0's code"
    code="$(cast code "$stop" --rpc-url "$rpc" | tr 'A-F' 'a-f')"
    wrapped="$(printf '%s' "${groth16#0x}" | tr 'A-F' 'a-f')"
    case "$code" in *"$wrapped"*) ;; *) fail "emergency stop $stop: its code does not hold the verifier it names" ;; esac
    [ "$(cast keccak "${code//$wrapped/0000000000000000000000000000000000000000}")" = "$(pin stopMaskedCodeHash)" ] ||
      fail "emergency stop $stop: not RISC Zero's emergency stop code"
    [ "$(call "$stop" 'owner()(address)')" = 0x0000000000000000000000000000000000000000 ] ||
      fail "emergency stop $stop: it has an owner, who can trip it and end releases and refunds for good"
    [ "$(call "$stop" 'paused()(bool)')" = false ] || fail "emergency stop $stop: tripped"
    echo "ok    Groth16 verifier $groth16: RISC Zero 3.1.0 (selector 0xe71e6620, code as pinned), behind RISC Zero's emergency stop $stop (owner renounced, not tripped)" ;;
  0x73c457ba)
    dcap="$(call "$vault" 'globalCap(uint8)(uint256,uint256,uint256)' 0 | head -n 1 | awk '{print $1}')"
    rcap="$(call "$vault" 'globalCap(uint8)(uint256,uint256,uint256)' 1 | head -n 1 | awk '{print $1}')"
    if [ "$dcap" = 0 ] && [ "$rcap" = 0 ]; then
      echo "ok    Groth16 verifier $groth16: selector 0x73c457ba (RISC Zero 3.0.0), and the vault's deposit and release caps are 0"
    else
      echo "FAIL  Groth16 verifier $groth16: selector 0x73c457ba (RISC Zero 3.0.0, forgeable) with caps not both 0: deposit $dcap, release $rcap"
      exit 1
    fi ;;
  *) echo "FAIL  Groth16 verifier $groth16: selector $sel"; exit 1 ;;
esac
