#!/usr/bin/env bash
# The Groth16 verifier behind a vault's emergency stop, read from the chain (nothing is sent):
#   SELECTOR() 0xe71e6620  RISC Zero's verifier 3.1.0: ok;
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
case "$sel" in
  0xe71e6620) echo "ok    Groth16 verifier $groth16: selector 0xe71e6620 (RISC Zero 3.1.0)" ;;
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
