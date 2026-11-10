// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Ownable} from "openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";
import {Receipt as RiscZeroReceipt} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {ToggleRecipient} from "./mocks/MockRecipients.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §6: the vault's verifier is RISC Zero's RiscZeroVerifierEmergencyStop (risc0-ethereum v3.0.0,
///         unmodified) around the base verifier, with the freeze authority as guardian. A stop is one-way: nothing
///         more is accepted, while paths that need no proof keep working.
contract EmergencyStopTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root");

    function test_estop_stopsCandidatesFinalisationReleasesAndRefunds_notCancellationClaimsOrRoles() public {
        // Leaves 0 and 1 are imported on LEZ (one is released before the stop, to a payee that cannot receive);
        // leaf 2 never is, and is refundable after the sunset.
        _deposit(alice, ETH, 1, keccak256("c0"));
        _deposit(alice, ETH, 1, keccak256("c1"));
        _deposit(bob, ETH, 0, keccak256("c2"));
        bytes32[] memory leaves = new bytes32[](3);
        leaves[0] = ReferenceMerkle.leafHash(0, alice, ETH, 1 ether, keccak256("c0"));
        leaves[1] = ReferenceMerkle.leafHash(1, alice, ETH, 1 ether, keccak256("c1"));
        leaves[2] = ReferenceMerkle.leafHash(2, bob, ETH, 0.1 ether, keccak256("c2"));
        vm.prank(freezer);
        vault.retire();
        FerrataVault.Checkpoint memory sunset = _nextCheckpoint(vault, ROOT, 2, true);
        _submit(sunset);
        _finalizeAfterDelta(sunset); // checkpoint 0
        vm.warp(block.timestamp + REVOKE_WINDOW + 1); // refunds open (v3 D5, D11)
        _acceptRoot(keccak256("later root")); // checkpoint 1, still revocable after the stop

        ToggleRecipient payee = new ToggleRecipient(); // cannot receive yet
        _release(_request(ETH, 1, address(payee), ROOT, keccak256("paid before the stop")));
        assertEq(vault.owed(address(payee), ETH), 1 ether);

        FerrataVault.Checkpoint memory pending = _nextCheckpoint(keccak256("next root"));
        _submit(pending); // submitted before the stop
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, keccak256("n"));
        _proveRelease(_releaseJournal(r));

        vm.prank(freezer);
        estop.estop();
        assertTrue(estop.paused());

        FerrataVault.Checkpoint memory fresh = _nextCheckpoint(keccak256("another root"));
        _proveCheckpoint(_checkpointJournal(fresh));
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.submitCandidate(fresh, SEAL);

        vm.warp(block.timestamp + DELTA + (uint256(T) - 1) * TURN_SECONDS); // the pending candidate's wait
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon(); // beacons need no proof
        (, bytes32 nonce) = vault.beacons(beaconIndex);
        pending.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(pending));
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.finalize(beaconIndex, pending, SEAL);

        vm.expectRevert(Pausable.EnforcedPause.selector);
        vault.release(r, SEAL);
        assertFalse(vault.nullifierUsed(r.nullifier));

        // No proof involved: cancellation, owed claims, revocation and role changes still work; refunds do not (D13).
        vm.prank(freezer);
        vault.cancelCandidate(pending.newState);
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, 2);
        bytes32[32] memory path;
        for (uint256 h = 0; h < 32; h++) {
            path[h] = siblings[h];
        }
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vault.refund(2, bob, ETH, 0, keccak256("c2"), path);
        assertFalse(vault.refunded(2));

        payee.setOpen(true);
        payee.claim(address(vault), ETH);
        assertEq(address(payee).balance, 1 ether);

        vm.prank(freezer);
        vault.revokeFrom(1);
        assertEq(vault.finalState(), sunset.newState);

        address newAdmin = makeAddr("newAdmin");
        vm.prank(admin);
        vault.transferAdmin(newAdmin);
        vm.prank(newAdmin);
        vault.acceptAdmin();
        assertEq(vault.admin(), newAdmin);
        address newFreezer = makeAddr("newFreezer");
        vm.prank(newAdmin);
        vault.proposeRoles([watcher, address(0), address(0)], newFreezer);
        vm.warp(block.timestamp + 14 days);
        vault.applyRoles();
        assertEq(vault.freezeAuthority(), newFreezer);
    }

    function test_estop_onlyGuardianOrAProofOfExploit() public {
        vm.prank(admin);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, admin));
        estop.estop();

        vm.expectRevert(RiscZeroVerifierEmergencyStop.InvalidProofOfExploit.selector);
        estop.estop(RiscZeroReceipt({seal: SEAL, claimDigest: bytes32(uint256(1))}));

        // Anyone holding a verifying receipt for the all-zero claim digest can stop it.
        verifier.acceptClaim(bytes32(0));
        vm.prank(alice);
        estop.estop(RiscZeroReceipt({seal: SEAL, claimDigest: bytes32(0)}));
        assertTrue(estop.paused());
    }

    function test_estop_cannotBeUndone() public {
        vm.prank(freezer);
        estop.estop();
        // The wrapper exposes no unpause, and a second stop changes nothing.
        (bool ok,) = address(estop).call(abi.encodeWithSignature("unpause()"));
        assertFalse(ok);
        vm.prank(freezer);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        estop.estop();
        assertTrue(estop.paused());
    }
}
