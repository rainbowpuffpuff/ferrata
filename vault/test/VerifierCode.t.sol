// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

/// @notice The code hashes script/check-verifier.sh holds a deployed vault's verifier to (script/params/verifier-31.json),
///         computed here from vault/lib.lock's risc0-ethereum v3.1.0 with the vault's compiler settings, the build
///         script/DeployVault.s.sol deploys (internal review of 27 November 2026, S-3): RISC Zero's Groth16 verifier
///         3.1.0 as deployed (its immutables are ControlID's), and RISC Zero's emergency stop with its one immutable, the
///         wrapped verifier's address, masked out (every occurrence of the 20 bytes replaced by zeros), so that one
///         hash holds for a stop around any verifier.
contract VerifierCodeTest is Test {
    string internal constant PINS = "script/params/verifier-31.json";

    /// @dev `code` with every occurrence of `a`'s 20 bytes zeroed, and how many there were.
    function _masked(bytes memory code, address a) internal pure returns (bytes memory out, uint256 n) {
        out = bytes.concat(code);
        bytes20 w = bytes20(a);
        for (uint256 i = 0; i + 20 <= out.length; i++) {
            bool same = true;
            for (uint256 j = 0; j < 20; j++) {
                if (out[i + j] != w[j]) {
                    same = false;
                    break;
                }
            }
            if (same) {
                for (uint256 j = 0; j < 20; j++) {
                    out[i + j] = 0;
                }
                n++;
                i += 19;
            }
        }
    }

    function test_theGroth16VerifiersCodeHashIsPinned() public {
        address g = address(new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID));
        assertEq(g.codehash, vm.parseJsonBytes32(vm.readFile(PINS), ".groth16CodeHash"), "the pin is v3.1.0's build");
    }

    function test_theStopsMaskedCodeHashIsPinnedAndHoldsForAnyWrappedVerifier() public {
        bytes32 pin = vm.parseJsonBytes32(vm.readFile(PINS), ".stopMaskedCodeHash");
        address[2] memory wrapped = [makeAddr("a verifier"), makeAddr("another verifier")];
        for (uint256 i = 0; i < 2; i++) {
            RiscZeroVerifierEmergencyStop s =
                new RiscZeroVerifierEmergencyStop(IRiscZeroVerifierFull(wrapped[i]), address(this));
            (bytes memory masked, uint256 n) = _masked(address(s).code, wrapped[i]);
            assertGt(n, 0, "the immutable is in the code");
            assertEq(keccak256(masked), pin, "the pin is the stop's code, its immutable masked");
        }
    }
}
