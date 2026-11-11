// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {VerificationFailed} from "risc0/IRiscZeroVerifier.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {RealBlocksFixture} from "./RealBlocks.t.sol";

/// @notice Rehearsal R1 of the crown run's proofs (e2e/crown/README.md, stages 7 and 9): the CROWN build's checkpoint_b
///         (its image id at the crown build path; sources with the ImportRoot decoding fix) proven on one NVIDIA L4 over
///         the lab's real LEZ blocks 8..=202, with a STAND-IN Sepolia configuration (chain id 11155111, the crown plan's
///         predicted vault address, labelled bridge fields), wrapped to Groth16; and the crown build's release program on
///         a STAND-IN synthetic burn, proven on the build box's CPU and wrapped. The vault is v5 in mode (B) with RISC
///         Zero's unmodified Groth16 verifier behind its emergency stop.
contract CrownR1Test is RealBlocksFixture {
    bytes32 internal constant CROWN_CHECKPOINT_B_IMAGE_ID =
        0x7e51065ffd0a8004fbdd0452871c4a77da7b59e6499365f43d61363fd4307754;
    bytes32 internal constant CROWN_RELEASE_IMAGE_ID = 0x20595dc54bbd80cf487e4666286db9aab7a048e2e0d7646b42ec6d3c90ca2625;

    bytes internal releaseJournal;
    bytes internal releaseSeal;

    function setUp() public {
        string memory json = vm.readFile("test/real/fixtures/crown-r1.json");
        require(vm.parseJsonBytes32(json, ".imageId") == CROWN_CHECKPOINT_B_IMAGE_ID, "image id");
        require(vm.parseJsonBytes32(json, ".releaseImageId") == CROWN_RELEASE_IMAGE_ID, "release image id");
        journal = vm.parseJsonBytes(json, ".journal");
        seal = vm.parseJsonBytes(json, ".seal");
        labKey = vm.parseJsonBytes32(json, ".labChannelKey");
        postingTimeframe = uint32(vm.parseJsonUint(json, ".postingTimeframe"));
        configTipHash = vm.parseJsonBytes32(json, ".configTipHash");
        parentTipId = uint64(vm.parseJsonUint(json, ".parentTipId"));
        parentTipHash = vm.parseJsonBytes32(json, ".parentTipHash");
        tipId = uint64(vm.parseJsonUint(json, ".tipId"));
        tipHash = vm.parseJsonBytes32(json, ".tipHash");
        releaseJournal = vm.parseJsonBytes(json, ".releaseJournal");
        releaseSeal = vm.parseJsonBytes(json, ".releaseSeal");
        _deploy(FerrataVault.Mode.B, CROWN_CHECKPOINT_B_IMAGE_ID, _state(_keys(labKey)), 1, 1);
    }

    /// @notice The vault's genesis (recomputed from the lab channel's key and block 7) is the proof's parent; the proof
    ///         is bound to Sepolia's chain id and the vault address the journal names.
    function test_crownR1_theProvenParentIsTheVaultsGenesis() public view {
        FerrataVault.Checkpoint memory c = _checkpoint();
        assertEq(c.parentState, vault.genesisState());
        assertEq(block.chainid, 11155111);
        assertEq(c.tipId, tipId);
        assertEq(c.tipHash, tipHash);
        assertEq(c.n, 1);
        assertEq(c.t, 1);
    }

    function test_crownR1_candidateThenFinalAfterTheWait() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.prank(makeAddr("anyone"));
        vault.submitCandidate(c, seal);
        uint256 at = block.timestamp;
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vault.finalizeAttested(c);
        vm.warp(at + WAIT);
        vault.finalizeAttested(c);
        assertEq(vault.finalState(), c.newState);
        (bool accepted,) = vault.burnRootStatus(c.burnRoot);
        assertTrue(accepted);
    }

    function test_crownR1_aFlippedSealByteIsRefused() public {
        FerrataVault.Checkpoint memory c = _checkpoint();
        bytes memory bad = bytes.concat(seal);
        bad[100] ^= 0x01;
        vm.expectRevert();
        vault.submitCandidate{gas: TAMPERED_CALL_GAS}(c, bad);
    }

    /// @notice The seal is bound to the crown image: a vault pinning the real-blocks proof's image id refuses it.
    function test_crownR1_aVaultOfTheEarlierImageRefusesIt() public {
        _deploy(FerrataVault.Mode.B, CHECKPOINT_B_IMAGE_ID, _state(_keys(labKey)), 1, 1);
        FerrataVault.Checkpoint memory c = _checkpoint();
        vm.expectRevert(VerificationFailed.selector);
        vault.submitCandidate(c, seal);
    }

    /// @notice The release seal verifies for the crown release image and its journal, and for nothing else.
    function test_crownR1_theReleaseSealVerifiesForTheCrownReleaseImage() public {
        RiscZeroGroth16Verifier groth16 = new RiscZeroGroth16Verifier(ControlID.CONTROL_ROOT, ControlID.BN254_CONTROL_ID);
        bytes32 digest = sha256(releaseJournal); // before expectRevert: the precompile call would be the "next call"
        groth16.verify(releaseSeal, CROWN_RELEASE_IMAGE_ID, digest);
        vm.expectRevert(VerificationFailed.selector);
        groth16.verify(releaseSeal, CROWN_CHECKPOINT_B_IMAGE_ID, digest);
    }
}
