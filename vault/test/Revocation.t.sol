// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §4.4 with v3 D4/D5: reject-only revocation of finalised checkpoints by the freeze authority.
contract RevocationTest is VaultTestBase {
    bytes32 internal constant ROOT_A = keccak256("burn root A");
    bytes32 internal constant ROOT_B = keccak256("burn root B");
    bytes32 internal constant ROOT_C = keccak256("burn root C");
    bytes32 internal constant N1 = keccak256("nullifier 1");
    bytes32 internal constant N2 = keccak256("nullifier 2");

    function _revoked(uint256 index) internal view returns (bool revoked) {
        (,,,,,, revoked,,,,) = vault.checkpoints(index);
    }

    function _accepted(bytes32 root) internal view returns (bool accepted) {
        (accepted,) = vault.burnRootStatus(root);
    }

    function test_revokeFrom_insideTheWindow() public {
        _acceptRoot(ROOT_A);
        bytes32 stateA = vault.finalState();
        _acceptRoot(ROOT_B);

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.CheckpointsRevoked(1, 1, stateA);
        vm.prank(freezer);
        vault.revokeFrom(1);

        assertEq(vault.finalState(), stateA);
        assertFalse(_revoked(0));
        assertTrue(_revoked(1));
        assertTrue(_accepted(ROOT_A));
        assertFalse(_accepted(ROOT_B));
    }

    function test_revokeFrom_marksEveryLaterCheckpoint() public {
        _acceptRoot(ROOT_A);
        _acceptRoot(ROOT_B);
        _acceptRoot(ROOT_C);
        vm.prank(freezer);
        vault.revokeFrom(0);
        assertEq(vault.finalState(), GENESIS_STATE);
        for (uint256 i = 0; i < 3; i++) {
            assertTrue(_revoked(i));
        }
        assertFalse(_accepted(ROOT_A));
        assertFalse(_accepted(ROOT_C));
    }

    function test_revokeFrom_onlyWithinTheWindow() public {
        uint256 index = _acceptRoot(ROOT_A);
        (,,,,,,,,, uint64 finalizedAt,) = vault.checkpoints(index);

        uint256 snapshot = vm.snapshotState();
        vm.warp(finalizedAt + REVOKE_WINDOW); // the last second of the window
        vm.prank(freezer);
        vault.revokeFrom(index);
        assertTrue(_revoked(index));
        vm.revertToState(snapshot);

        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.RevokeWindowPassed.selector);
        vault.revokeFrom(index);
        assertTrue(_accepted(ROOT_A));
    }

    function test_revokeFrom_onlyTheFreezeAuthority() public {
        _acceptRoot(ROOT_A);
        address[3] memory others = [admin, alice, relayer];
        for (uint256 i = 0; i < others.length; i++) {
            vm.prank(others[i]);
            vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
            vault.revokeFrom(0);
        }
    }

    function test_revokeFrom_unknownOrAlreadyRevoked() public {
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.UnknownCheckpoint.selector);
        vault.revokeFrom(0);

        _acceptRoot(ROOT_A);
        _acceptRoot(ROOT_B);
        vm.prank(freezer);
        vault.revokeFrom(0);
        // Revoking an already revoked checkpoint again would restore a revoked state; it is refused.
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.CheckpointAlreadyRevoked.selector);
        vault.revokeFrom(1);
    }

    function test_releaseAgainstARevokedRootReverts() public {
        _deposit(alice, ETH, 1, keccak256("c"));
        _acceptRoot(ROOT_A);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT_A, N1);
        _proveRelease(_releaseJournal(r));
        vm.prank(freezer);
        vault.revokeFrom(0);
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(r, SEAL);
    }

    function test_nullifiersUsedBeforeARevocationStayUsed() public {
        _deposit(alice, ETH, 1, keccak256("c1"));
        _deposit(alice, ETH, 1, keccak256("c2"));
        _acceptRoot(ROOT_A);
        _release(_request(ETH, 1, recipient, ROOT_A, N1));
        vm.prank(freezer);
        vault.revokeFrom(0);
        assertTrue(vault.nullifierUsed(N1));
        assertEq(vault.escrowed(ETH), 1 ether + 2 * (1 ether / 500)); // the release that happened stays a release
    }

    /// @notice After a revocation the same LEZ state can be proven again: it needs a fresh candidate and a full
    ///         delta. Its root is then accepted again, and a nullifier already paid cannot be paid a second time.
    function test_reprovenCheckpointIsAcceptedAgain_butANullifierPaysOnce() public {
        _deposit(alice, ETH, 1, keccak256("c1"));
        _deposit(alice, ETH, 1, keccak256("c2"));
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT_A);
        _submit(c);
        _finalizeAfterDelta(c);
        _release(_request(ETH, 1, recipient, ROOT_A, N1));

        vm.prank(freezer);
        vault.revokeFrom(0);
        assertFalse(_accepted(ROOT_A));

        c.echoNonce = bytes32(0);
        _submit(c); // the same state and root, proven again
        assertFalse(_accepted(ROOT_A)); // not before a new delta
        uint256 index = _finalizeAfterDelta(c);
        assertEq(index, 1);
        assertTrue(_accepted(ROOT_A));
        assertEq(vault.burnRootCheckpoint(ROOT_A), 2);

        FerrataVault.ReleaseRequest memory again = _request(ETH, 1, recipient, ROOT_A, N1);
        _proveRelease(_releaseJournal(again));
        vm.expectRevert(FerrataVault.NullifierAlreadyUsed.selector);
        vault.release(again, SEAL);

        _release(_request(ETH, 1, recipient, ROOT_A, N2));
        assertEq(recipient.balance, 2 ether);
    }

    function test_revokeFrom_invalidatesPendingCandidates() public {
        _acceptRoot(ROOT_A);
        FerrataVault.Checkpoint memory pending = _nextCheckpoint(ROOT_B); // built on A
        _submit(pending);
        vm.prank(freezer);
        vault.revokeFrom(0);

        vm.warp(block.timestamp + DELTA);
        vm.roll(block.number + 1);
        uint256 b = vault.recordBeacon();
        (, bytes32 nonce) = vault.beacons(b);
        pending.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(pending));
        vm.expectRevert(FerrataVault.StaleCandidate.selector);
        vault.finalize(b, pending, SEAL);
    }

    function test_revokeFrom_acceptsNothing() public {
        _acceptRoot(ROOT_A);
        _acceptRoot(ROOT_B);
        uint256 checkpointsBefore = vault.checkpointCount();
        vm.prank(freezer);
        vault.revokeFrom(1);
        assertEq(vault.checkpointCount(), checkpointsBefore); // history is kept, only marked
        assertFalse(_accepted(ROOT_B));
        assertTrue(_accepted(ROOT_A));
        assertEq(vault.burnRootCheckpoint(ROOT_B), 2);
    }

    /// @notice v3 D4: a revocation restores the imported leaf count and sunset flag of the state it returns to, so the
    ///         monotonicity checks continue from there; a revoked checkpoint's count or sunset binds nothing later.
    function test_revokeFrom_restoresTheMonotonicityState() public {
        _fillTree(16); // v4 (D11): imported counts stay within the tree
        _acceptRoot(ROOT_A); // checkpoint 0: count 0
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT_B, 5, false);
        _submit(c);
        _finalizeAfterDelta(c); // checkpoint 1: count 5
        c = _nextCheckpoint(vault, ROOT_C, 7, true);
        _submit(c);
        _finalizeAfterDelta(c); // checkpoint 2: sunset at count 7
        assertEq(vault.finalImportedLeafCount(), 7);
        assertTrue(vault.finalSunset());

        vm.prank(freezer);
        vault.revokeFrom(2);
        assertEq(vault.finalImportedLeafCount(), 5);
        assertFalse(vault.finalSunset());
        vm.prank(freezer);
        vault.revokeFrom(1);
        assertEq(vault.finalImportedLeafCount(), 0);
        assertFalse(vault.finalSunset());

        c = _nextCheckpoint(vault, ROOT_B, 3, false); // below both revoked counts, and no sunset: fine again
        _submit(c);
        uint256 index = _finalizeAfterDelta(c);
        assertEq(vault.finalImportedLeafCount(), 3);
        (,,,,,,, uint64 parentImported, bool parentSunset,,) = vault.checkpoints(index);
        assertEq(parentImported, 0);
        assertFalse(parentSunset);
    }

    /// @notice Revoking a later checkpoint returns to a state that is already a sunset one, so the sunset stays final
    ///         and a checkpoint without it is still refused.
    function test_revokeFrom_keepsTheSunsetOfTheRestoredState() public {
        _fillTree(16); // v4 (D11): imported counts stay within the tree
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT_A, 4, true);
        _submit(c);
        _finalizeAfterDelta(c); // checkpoint 0: sunset at count 4
        _acceptRoot(ROOT_B); // checkpoint 1: a sunset checkpoint with the same count
        (,,,,,,, uint64 parentImported, bool parentSunset,,) = vault.checkpoints(1);
        assertEq(parentImported, 4);
        assertTrue(parentSunset);
        vm.prank(freezer);
        vault.revokeFrom(1);
        assertTrue(vault.finalSunset());
        assertEq(vault.finalImportedLeafCount(), 4);

        c = _nextCheckpoint(vault, ROOT_C, 4, false);
        _submit(c);
        (uint256 submittedAt, uint256 wait) = _waitOf(vault, c.newState);
        vm.warp(submittedAt + wait);
        vm.roll(block.number + 1);
        uint256 b = vault.recordBeacon();
        (, c.echoNonce) = vault.beacons(b);
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.SunsetIsFinal.selector);
        vault.finalize(b, c, SEAL);
    }
}
