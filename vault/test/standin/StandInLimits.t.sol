// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

// The limits of the stand-in arrangement, from an independent review (18 Nov 2026): odd recovery ids, the vault's
// refusal of equal image ids (what keeps every release on a real proof), and no replay or second payment.
//   forge test --match-path test/standin/StandInLimits.t.sol

import {FerrataVault} from "../../src/FerrataVault.sol";
import {StandInCheckpointVerifier} from "../../src/standin/StandInCheckpointVerifier.sol";
import {StandInVaultTest} from "./StandInVault.t.sol";

contract StandInLimitsTest is StandInVaultTest {
    /// A raw recovery id (v = 0 or 1, as some signers emit) or any v other than 27/28 is refused, not mis-recovered.
    function test_aRawOrOddRecoveryIdIsRefused() public {
        bytes32 j = keccak256("limits: a journal digest");
        bytes32 d = _digest(block.chainid, address(standIn), standInId, j);
        (uint8 v0, bytes32 r0, bytes32 s0) = vm.sign(keys[0], d);
        (uint8 v1, bytes32 r1, bytes32 s1) = vm.sign(keys[1], d);
        // Sanity: as signed (27/28), the pair verifies.
        standIn.verify(bytes.concat(abi.encodePacked(r0, s0, v0), abi.encodePacked(r1, s1, v1)), standInId, j);
        uint8[4] memory odd = [uint8(v0 - 27), uint8(v0 + 2), uint8(0), uint8(255)];
        for (uint256 i = 0; i < odd.length; i++) {
            bytes memory seal = bytes.concat(abi.encodePacked(r0, s0, odd[i]), abi.encodePacked(r1, s1, v1));
            vm.expectRevert(StandInCheckpointVerifier.InvalidSignature.selector);
            standIn.verify(seal, standInId, j);
        }
    }

    /// The stand-in itself cannot tell a checkpoint digest from a release digest: whatever digest the keys sign under
    /// STAND_IN_IMAGE_ID verifies. What keeps releases on real proofs is the vault: it refuses equal image ids, and
    /// DeployStandIn pins checkpointImageId = STAND_IN_IMAGE_ID (verify.py re-checks releaseImageId).
    function test_theVaultRefusesTheStandInIdForBothPrograms() public {
        FerrataVault.Deployment memory d = _deployment();
        d.releaseImageId = standInId; // checkpointImageId is already the stand-in id
        vm.expectRevert(FerrataVault.InvalidImageIds.selector);
        new FerrataVault(d);
        // The guard is the vault's: with another checkpoint id, a vault naming the stand-in id for releases builds,
        // and the stand-in answers a signed RELEASE digest. DeployStandIn never builds this (checkpoint id pinned).
        d.checkpointImageId = CHECKPOINT_B_IMAGE_ID;
        FerrataVault misdeployed = new FerrataVault(d);
        assertEq(misdeployed.releaseImageId(), standInId);
        bytes32 releaseDigest = sha256("limits: any release journal");
        standIn.verify(_seal(releaseDigest), standInId, releaseDigest);
    }

    /// A finalised signed journal cannot be submitted again (its parent is no longer final); a later checkpoint that
    /// re-attests the same root does not re-open a paid nullifier (the nullifier is the burn's, not the root's).
    function test_noReplay_andNoSecondPaymentUnderALaterCheckpoint() public {
        FerrataVault.Checkpoint memory c1 = _finalise();
        bytes memory seal1 = _seal(_journalDigest(c1));
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(c1, seal1);

        (, uint256 finalizedAt) = vault.burnRootStatus(burnRoot);
        vm.warp(finalizedAt + RELEASE_DELAY);
        (FerrataVault.ReleaseRequest memory r0, bytes memory seal0) = _release(0);
        vault.release(r0, seal0);

        // A second checkpoint on top, signed by the keys, carrying the same root.
        FerrataVault.Checkpoint memory c2 = c1;
        c2.parentState = c1.newState;
        c2.newState = keccak256("limits: a later state, same burns");
        c2.tipId = c1.tipId + 1;
        vault.submitCandidate(c2, _seal(_journalDigest(c2)));
        vm.warp(block.timestamp + DELTA);
        vault.finalizeAttested(c2);
        assertEq(vault.finalState(), c2.newState);
        vm.warp(block.timestamp + RELEASE_DELAY);
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(r0, seal0);
        // The unpaid burn is still paid once.
        (FerrataVault.ReleaseRequest memory r1, bytes memory seal1b) = _release(1);
        vault.release(r1, seal1b);
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(r1, seal1b);
    }
}
