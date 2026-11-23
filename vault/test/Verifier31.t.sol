// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier, SelectorMismatch} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

/// @notice The verifier a vault is deployed against since the move to RISC Zero 3.1 (docs/design/risc0-3.1-migration.md;
///         RISC Zero's advisories GHSA-g6qg-9j8p-7j87 and GHSA-mm6c-35cm-g4x2, docs/security/risc0-groth16-advisory-2026-11-21):
///         risc0-ethereum v3.1.0 (commit ff30da77, vault/lib.lock), whose RiscZeroGroth16Verifier is version 3.1.0 with
///         selector 0xe71e6620, behind RISC Zero's unmodified RiscZeroVerifierEmergencyStop (script/DeployVault.s.sol).
///         A seal made by risc0 3.0.5 (selector 0x73c457ba, the stopped verifier 3.0.0's) is refused before any pairing.
contract Verifier31Test is Test {
    /// risc0-ethereum v3.1.0, contracts/src/groth16/ControlID.sol and RiscZeroGroth16Verifier.sol.
    bytes4 internal constant SELECTOR_3_1_0 = 0xe71e6620;
    bytes32 internal constant CONTROL_ROOT_3_1_0 = 0xe6feef13db7aed4a731c7b0852de1f5c89b137726073f625d05333184d3a551d;
    bytes32 internal constant BN254_CONTROL_ID_3_1_0 = 0x0ac2eaf9d5ae64ffb75c4747fde32456e9ee6db3b32350cb2a0614ba91d081de;
    /// The selector of verifier 3.0.0 and of every Groth16 seal of risc0-zkvm 3.0.5 (the stopped one).
    bytes4 internal constant SELECTOR_3_0_0 = 0x73c457ba;
    /// release, docs/image-ids.md (RISC Zero 3.1).
    bytes32 internal constant RELEASE_IMAGE_ID = 0xf279af414a8beef160a5a58cef741eba0269b7635a0ccc3d5d22e3c5d771a6fd;

    RiscZeroGroth16Verifier internal groth16;
    RiscZeroVerifierEmergencyStop internal stop;

    function setUp() public {
        groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        stop = new RiscZeroVerifierEmergencyStop(groth16, address(this));
    }

    /// @notice The library is verifier 3.1.0: its control root and BN254 control id, its version, its selector.
    function test_theLibraryIsVerifier3_1_0() public view {
        assertEq(ControlID.CONTROL_ROOT, CONTROL_ROOT_3_1_0);
        assertEq(ControlID.BN254_CONTROL_ID, BN254_CONTROL_ID_3_1_0);
        assertEq(groth16.VERSION(), "3.1.0");
        assertEq(groth16.SELECTOR(), SELECTOR_3_1_0);
        assertEq(address(stop.verifier()), address(groth16));
    }

    /// @notice A seal of risc0 3.0.5 (crown-live-3's first release, Sepolia, 12 November 2026) is refused by the
    ///         verifier and through the stop: SelectorMismatch, whatever image and journal it is sent for.
    function test_aSealOfRisc0_3_0_5IsRefused() public {
        string memory run = vm.readFile("../e2e/runs/crown-live-3/crown.json");
        bytes memory old = vm.parseJsonBytes(run, ".releases[0].seal");
        assertEq(old.length, 260);
        assertEq(bytes4(old), SELECTOR_3_0_0);
        bytes32 digest = sha256(vm.parseJsonBytes(vm.readFile("test/real/fixtures/crown-live-3-releases.json"), ".releases[0].journal"));
        bytes memory expected = abi.encodeWithSelector(SelectorMismatch.selector, SELECTOR_3_0_0, SELECTOR_3_1_0);
        vm.expectRevert(expected);
        groth16.verify(old, RELEASE_IMAGE_ID, digest);
        vm.expectRevert(expected);
        stop.verify(old, RELEASE_IMAGE_ID, digest);
    }

    /// @notice The same release proven again under RISC Zero 3.1 verifies, for its journal and the release program only.
    function test_theSameReleaseProvenUnder3_1Verifies() public {
        string memory fx = vm.readFile("test/real/fixtures/crown-live-3-releases.json");
        assertEq(vm.parseJsonBytes32(fx, ".releaseImageId"), RELEASE_IMAGE_ID);
        bytes memory seal = vm.parseJsonBytes(fx, ".releases[0].seal");
        bytes32 digest = sha256(vm.parseJsonBytes(fx, ".releases[0].journal"));
        assertEq(bytes4(seal), SELECTOR_3_1_0);
        groth16.verify(seal, RELEASE_IMAGE_ID, digest);
        stop.verify(seal, RELEASE_IMAGE_ID, digest);
        bytes32 other = sha256("another journal"); // before expectRevert: SHA-256 is a call to a precompile
        vm.expectRevert(VerificationFailed.selector);
        stop.verify(seal, RELEASE_IMAGE_ID, other);
        vm.expectRevert(VerificationFailed.selector);
        stop.verify(seal, bytes32(uint256(RELEASE_IMAGE_ID) ^ 1), digest);
    }

    /// @notice A seal made by the prover service on a GPU (STARK and Groth16 step both on an NVIDIA L4,
    ///         gpu-prover/modal_app.py prove_bytes) verifies like the CPU-made one, for its journal only.
    function test_aSealMadeOnTheGpuVerifies() public {
        string memory fx = vm.readFile("test/real/fixtures/gpu-release-31.json");
        assertEq(vm.parseJsonBytes32(fx, ".releaseImageId"), RELEASE_IMAGE_ID);
        bytes memory seal = vm.parseJsonBytes(fx, ".seal");
        bytes32 digest = sha256(vm.parseJsonBytes(fx, ".journal"));
        bytes32 other = sha256("another journal");
        assertEq(bytes4(seal), SELECTOR_3_1_0);
        stop.verify(seal, RELEASE_IMAGE_ID, digest);
        vm.expectRevert(VerificationFailed.selector);
        stop.verify(seal, RELEASE_IMAGE_ID, other);
    }

    /// @notice A seal whose selector is 3.1.0's but whose proof is another's is refused (the second release's proof
    ///         under the first's journal).
    function test_aSwappedProofIsRefused() public {
        string memory fx = vm.readFile("test/real/fixtures/crown-live-3-releases.json");
        bytes memory other = vm.parseJsonBytes(fx, ".releases[1].seal");
        bytes32 digest = sha256(vm.parseJsonBytes(fx, ".releases[0].journal"));
        vm.expectRevert(VerificationFailed.selector);
        stop.verify(other, RELEASE_IMAGE_ID, digest);
    }
}
