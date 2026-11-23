// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {DeployVault} from "../script/DeployVault.s.sol";

/// @notice Answers RiscZeroGroth16Verifier 3.1.0's VERSION() and SELECTOR() and accepts every seal: what a check of those
///         two values alone would let behind the stop (FERRATA_GROTH16_VERIFIER, or an inherited one).
contract ImpostorGroth16Verifier {
    string public constant VERSION = "3.1.0";
    bytes4 public constant SELECTOR = 0xe71e6620;

    function verify(bytes calldata, bytes32, bytes32) external pure {}
}

contract DeployVaultHarness is DeployVault {
    function checkVerifier(address groth16) external {
        _checkVerifier(groth16);
    }
}

/// @notice script/DeployVault.s.sol puts a Groth16 verifier behind the stop only if it is RISC Zero's 3.1.0 built from
///         vault/lib.lock: its version, its selector and its code (review of bridge-31, L9).
contract DeployVaultChecksTest is Test {
    DeployVaultHarness internal h;

    function setUp() public {
        h = new DeployVaultHarness();
    }

    function test_theVerifierOfTheLockedLibraryPasses() public {
        h.checkVerifier(address(new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID)));
    }

    function test_anImpostorAnsweringVersionAndSelectorIsRefused() public {
        address impostor = address(new ImpostorGroth16Verifier());
        assertEq(RiscZeroGroth16Verifier(impostor).SELECTOR(), bytes4(0xe71e6620));
        vm.expectRevert(bytes("DeployVault: the Groth16 verifier's code is not risc0-ethereum v3.1.0's (vault/lib.lock)"));
        h.checkVerifier(impostor);
    }

    function test_aVerifierOfOtherControlIdsIsRefused() public {
        address other = address(new RiscZeroGroth16Verifier(bytes32(uint256(1)), bytes32(uint256(2))));
        vm.expectRevert(bytes("DeployVault: the Groth16 verifier's selector is not 0xe71e6620"));
        h.checkVerifier(other);
    }
}
