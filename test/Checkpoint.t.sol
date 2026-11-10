// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §4.3: candidate, beacon, finalisation.
contract CheckpointTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root 1");
    bytes32 internal constant ROOT_2 = keccak256("burn root 2");

    function _candidate(FerrataVault.Checkpoint memory c)
        internal
        view
        returns (bytes32 parent, uint256 submittedAt, uint256 deltaAtSubmit, uint256 epoch)
    {
        (parent,,,,, submittedAt, deltaAtSubmit, epoch) = vault.candidates(c.newState);
    }

    /// @dev Records a beacon in a new block at time `at` and returns its index and nonce.
    function _beaconAt(uint256 at) internal returns (uint256 index, bytes32 nonce) {
        vm.warp(at);
        vm.roll(block.number + 1);
        index = vault.recordBeacon();
        (, nonce) = vault.beacons(index);
    }

    /// @dev Proves the finalisation journal of `c` echoing `nonce`, then calls finalize with `beaconIndex`.
    function _tryFinalize(FerrataVault.Checkpoint memory c, uint256 beaconIndex, bytes32 nonce) internal {
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));
        vault.finalize(beaconIndex, c, SEAL);
    }

    // ---- beacons ----------------------------------------------------------------------------------------------------

    function test_recordBeacon_onePerBlock_nonceFromPrevrandao() public {
        vm.prevrandao(bytes32(uint256(123)));
        bytes32 expected = keccak256(abi.encode(uint256(123), block.number, address(vault)));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.BeaconRecorded(0, block.timestamp, expected);
        vm.prank(alice); // permissionless
        assertEq(vault.recordBeacon(), 0);
        assertEq(vault.recordBeacon(), 0); // same block: the same beacon, nothing appended
        assertEq(vault.beaconCount(), 1);

        vm.roll(block.number + 1);
        vm.warp(block.timestamp + 12);
        vm.prevrandao(bytes32(uint256(456)));
        assertEq(vault.recordBeacon(), 1);
        (uint256 timestamp, bytes32 nonce) = vault.beacons(1);
        assertEq(timestamp, block.timestamp);
        assertEq(nonce, keccak256(abi.encode(uint256(456), block.number, address(vault))));
    }

    // ---- submitCandidate --------------------------------------------------------------------------------------------

    function test_submitCandidate_storesTheCandidateAndRecordsABeacon() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.CandidateSubmitted(c.newState, GENESIS_STATE, ROOT, c.burnCount, 0, false, DELTA);
        vm.prank(relayer); // permissionless
        vault.submitCandidate(c, SEAL);

        (bytes32 parent, uint256 submittedAt, uint256 deltaAtSubmit, uint256 epoch) = _candidate(c);
        assertEq(parent, GENESIS_STATE);
        assertEq(submittedAt, block.timestamp);
        assertEq(deltaAtSubmit, DELTA);
        assertEq(epoch, 0);
        assertEq(vault.beaconCount(), 1);
        (bool accepted,) = vault.burnRootStatus(ROOT);
        assertFalse(accepted); // a candidate accepts nothing
        assertEq(vault.finalState(), GENESIS_STATE);
    }

    function test_submitCandidate_parentMustBeTheFinalState() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        c.parentState = keccak256("some other state");
        _proveCheckpoint(_checkpointJournal(c)); // even with a valid proof
        vm.expectRevert(FerrataVault.ParentNotFinal.selector);
        vault.submitCandidate(c, SEAL);
    }

    function test_submitCandidate_echoNonceMustBeZero() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        c.echoNonce = keccak256("an echo");
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.EchoNonceNotZero.selector);
        vault.submitCandidate(c, SEAL);
    }

    function test_submitCandidate_committeeFloorMustBeTheDeployedOne() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        c.nMin = N_MIN - 1;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.submitCandidate(c, SEAL);

        c.nMin = N_MIN;
        c.tMin = T_MIN - 1;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.submitCandidate(c, SEAL);
    }

    function test_submitCandidate_needsAValidProof() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.submitCandidate(c, SEAL);
    }

    function test_submitCandidate_resubmittingDoesNotRestartTheWait() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        (, uint256 submittedAt,,) = _candidate(c);
        vm.warp(block.timestamp + 1 hours);
        vm.expectRevert(FerrataVault.CandidateAlreadySubmitted.selector);
        vault.submitCandidate(c, SEAL); // the same public proof, replayed
        (, uint256 submittedAtAfter,,) = _candidate(c);
        assertEq(submittedAtAfter, submittedAt);
    }

    function test_submitCandidate_stoppedByTheCandidateFreeze() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _proveCheckpoint(_checkpointJournal(c));
        vm.prank(freezer);
        vault.setCandidatesFrozen(true);
        vm.expectRevert(FerrataVault.CandidatesAreFrozen.selector);
        vault.submitCandidate(c, SEAL);
        vm.prank(freezer);
        vault.setCandidatesFrozen(false);
        vault.submitCandidate(c, SEAL);
    }

    // ---- finalize: the finality rule ---------------------------------------------------------------------------------

    function test_finalize_acceptsTheBurnRoot() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + DELTA);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.CheckpointFinalized(0, c.newState, GENESIS_STATE, ROOT, c.burnCount, 0, false);
        vm.prank(relayer); // permissionless
        vault.finalize(b, c, SEAL);

        assertEq(vault.finalState(), c.newState);
        assertEq(vault.revocationEpoch(), 0); // only revocations change it
        assertEq(vault.checkpointCount(), 1);
        (
            bytes32 parent,
            bytes32 newState,
            bytes32 burnRoot,
            uint64 burnCount,
            uint64 imported,
            bool sunset,
            bool revoked,
            uint256 finalizedAt
        ) = vault.checkpoints(0);
        assertEq(parent, GENESIS_STATE);
        assertEq(newState, c.newState);
        assertEq(burnRoot, ROOT);
        assertEq(burnCount, c.burnCount);
        assertEq(imported, 0);
        assertFalse(sunset);
        assertFalse(revoked);
        assertEq(finalizedAt, block.timestamp);
        (bool accepted, uint256 at) = vault.burnRootStatus(ROOT);
        assertTrue(accepted);
        assertEq(at, block.timestamp);
        (,, uint256 submittedAt,) = _candidate(c);
        assertEq(submittedAt, 0); // the candidate record is gone
    }

    function test_finalize_impossibleBeforeDelta_noBeaconIsOldEnough() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        uint256 submittedAt = block.timestamp;

        // The only beacon so far is the candidate's own.
        (, bytes32 ownNonce) = vault.beacons(0);
        c.echoNonce = ownNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(0, c, SEAL);

        // One second short of delta.
        (uint256 early, bytes32 earlyNonce) = _beaconAt(submittedAt + DELTA - 1);
        c.echoNonce = earlyNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(early, c, SEAL);

        // Exactly delta later is enough.
        (uint256 onTime, bytes32 onTimeNonce) = _beaconAt(submittedAt + DELTA);
        _tryFinalize(c, onTime, onTimeNonce);
        assertEq(vault.finalState(), c.newState);
    }

    function test_finalize_rejectsABeaconRecordedBeforeTheCandidate() public {
        (uint256 old, bytes32 oldNonce) = _beaconAt(block.timestamp + 1 days);
        vm.warp(block.timestamp + 10 days);
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        c.echoNonce = oldNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(old, c, SEAL);
    }

    function test_finalize_echoNonceMustMatchTheNamedBeacon() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        (uint256 b1, bytes32 nonce1) = _beaconAt(block.timestamp + DELTA);
        (uint256 b2, bytes32 nonce2) = _beaconAt(block.timestamp + 12);
        assertTrue(nonce1 != nonce2);

        // A proof echoing beacon 2, submitted naming beacon 1.
        c.echoNonce = nonce2;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.EchoNonceMismatch.selector);
        vault.finalize(b1, c, SEAL);

        // A nonce that is no beacon's at all.
        c.echoNonce = keccak256("made up");
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.EchoNonceMismatch.selector);
        vault.finalize(b1, c, SEAL);

        _tryFinalize(c, b2, nonce2);
        assertEq(vault.finalState(), c.newState);
    }

    function test_finalize_unknownBeaconOrCandidate() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vault.finalize(0, c, SEAL);

        _submit(c);
        vm.expectRevert(FerrataVault.UnknownBeacon.selector);
        vault.finalize(5, c, SEAL);
    }

    function test_finalize_fieldsMustEqualTheCandidates() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + DELTA);
        c.echoNonce = nonce;

        for (uint256 field = 0; field < 5; field++) {
            FerrataVault.Checkpoint memory changed = _copy(c);
            if (field == 0) changed.parentState = keccak256("other parent");
            if (field == 1) changed.burnRoot = ROOT_2;
            if (field == 2) changed.burnCount = c.burnCount + 1;
            if (field == 3) changed.importedLeafCount = 7;
            if (field == 4) changed.sunset = true;
            _proveCheckpoint(_checkpointJournal(changed)); // a valid proof of the changed statement
            vm.expectRevert(FerrataVault.CandidateMismatch.selector);
            vault.finalize(b, changed, SEAL);
        }

        FerrataVault.Checkpoint memory floor = _copy(c);
        floor.tMin = T_MIN - 1;
        _proveCheckpoint(_checkpointJournal(floor));
        vm.expectRevert(FerrataVault.WrongCommitteeFloor.selector);
        vault.finalize(b, floor, SEAL);

        _tryFinalize(c, b, nonce);
    }

    function test_finalize_needsItsOwnProof() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c); // the candidate proof (echoNonce = 0) is registered
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + DELTA);
        c.echoNonce = nonce;
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        vault.finalize(b, c, SEAL);
    }

    function test_finalize_candidateWhoseParentIsNoLongerFinal() public {
        FerrataVault.Checkpoint memory a = _nextCheckpoint(ROOT);
        FerrataVault.Checkpoint memory b = _nextCheckpoint(ROOT_2); // also built on genesis
        _submit(a);
        _submit(b);
        _finalizeAfterDelta(a);
        assertEq(vault.finalState(), a.newState);

        (uint256 beacon, bytes32 nonce) = _beaconAt(block.timestamp + DELTA);
        b.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(b));
        vm.expectRevert(FerrataVault.StaleCandidate.selector);
        vault.finalize(beacon, b, SEAL);
    }

    /// @notice Two competing candidates on the same parent: once one is final, the other can never be finalised,
    ///         not even after the winner is revoked and the parent is final again (it must be submitted afresh and
    ///         wait a full delta).
    function test_finalize_competingCandidates_theLoserNeverFinalizes() public {
        FerrataVault.Checkpoint memory winner = _nextCheckpoint(ROOT);
        FerrataVault.Checkpoint memory loser = _nextCheckpoint(ROOT_2);
        _submit(winner);
        _submit(loser);
        _finalizeAfterDelta(winner);

        (uint256 beacon, bytes32 nonce) = _beaconAt(block.timestamp + 1 days);
        loser.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(loser));
        vm.expectRevert(FerrataVault.StaleCandidate.selector);
        vault.finalize(beacon, loser, SEAL);

        vm.prank(freezer);
        vault.revokeFrom(0);
        assertEq(vault.finalState(), GENESIS_STATE); // the loser's parent is final again ...
        vm.expectRevert(FerrataVault.StaleCandidate.selector);
        vault.finalize(beacon, loser, SEAL); // ... but its old wait does not count

        loser.echoNonce = bytes32(0);
        _submit(loser); // a fresh submission starts a fresh delta
        (, uint256 resubmittedAt,,) = _candidate(loser);
        assertEq(resubmittedAt, block.timestamp);
        _finalizeAfterDelta(loser);
        assertEq(vault.finalState(), loser.newState);
    }

    /// @notice Only revocations invalidate pending candidates. Finalising a no-op checkpoint (newState == parentState,
    ///         if the checkpoint program can prove one) therefore cannot be used to make honest candidates restart.
    function test_finalize_aNoOpCheckpointDoesNotInvalidatePendingCandidates() public {
        FerrataVault.Checkpoint memory noOp = _nextCheckpoint(ROOT);
        noOp.newState = noOp.parentState;
        FerrataVault.Checkpoint memory honest = _nextCheckpoint(ROOT_2);
        _submit(honest);
        _submit(noOp);
        _finalizeAfterDelta(noOp);
        assertEq(vault.finalState(), GENESIS_STATE);
        _finalizeAfterDelta(honest); // still valid, no resubmission needed
        assertEq(vault.finalState(), honest.newState);
    }

    function test_finalize_deltaRaiseDoesNotAffectASubmittedCandidate() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        uint256 submittedAt = block.timestamp;
        vm.prank(admin);
        vault.raiseDelta(DELTA * 3);

        (uint256 b, bytes32 nonce) = _beaconAt(submittedAt + DELTA); // the old delta is enough for this candidate
        _tryFinalize(c, b, nonce);
        assertEq(vault.finalState(), c.newState);
    }

    function test_finalize_deltaRaiseAppliesToLaterCandidates() public {
        vm.prank(admin);
        vault.raiseDelta(DELTA * 3);
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        uint256 submittedAt = block.timestamp;
        (,, uint256 deltaAtSubmit,) = _candidate(c);
        assertEq(deltaAtSubmit, DELTA * 3);

        (uint256 early, bytes32 earlyNonce) = _beaconAt(submittedAt + DELTA);
        c.echoNonce = earlyNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(early, c, SEAL);

        (uint256 late, bytes32 lateNonce) = _beaconAt(submittedAt + DELTA * 3);
        _tryFinalize(c, late, lateNonce);
    }

    function test_finalize_notStoppedByTheCandidateFreeze() public {
        // As specified, the freeze stops submissions only; a submitted candidate can still be finalised.
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        vm.prank(freezer);
        vault.setCandidatesFrozen(true);
        _finalizeAfterDelta(c);
        assertEq(vault.finalState(), c.newState);
    }

    function test_finalize_aChainOfCheckpoints() public {
        bytes32 parent = GENESIS_STATE;
        for (uint256 i = 0; i < 4; i++) {
            FerrataVault.Checkpoint memory c = _nextCheckpoint(keccak256(abi.encode("root", i)));
            assertEq(c.parentState, parent);
            _submit(c);
            uint256 index = _finalizeAfterDelta(c);
            assertEq(index, i);
            assertEq(vault.finalState(), c.newState);
            parent = c.newState;
        }
        assertEq(vault.revocationEpoch(), 0);
    }

    function test_finalize_aRootFinalisedTwiceKeepsItsFirstTime() public {
        _acceptRoot(ROOT);
        uint256 first = _finalizedAt(ROOT);
        vm.warp(block.timestamp + 1 days);
        _acceptRoot(ROOT); // no new burns: the next checkpoint carries the same root
        assertEq(_finalizedAt(ROOT), first);
        assertEq(vault.burnRootCheckpoint(ROOT), 1);
    }

    function test_finalize_sunsetCheckpointSetsTheRefundBoundary() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, 5, true);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + DELTA);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.SunsetLeafCountSet(0, 5);
        vault.finalize(b, c, SEAL);
        assertTrue(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetLeafCount(), 5);
        assertEq(vault.sunsetSetAt(), 0);
    }

    // ---- delta ---------------------------------------------------------------------------------------------------

    function test_raiseDelta_onlyUpAndBelowTheCeiling() public {
        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.DeltaCanOnlyRise.selector);
        vault.raiseDelta(DELTA - 1);
        uint256 ceiling = vault.MAX_DELTA();
        vm.expectRevert(FerrataVault.DeltaTooLong.selector);
        vault.raiseDelta(ceiling + 1);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.DeltaSet(DELTA + 1);
        vault.raiseDelta(DELTA + 1);
        vm.stopPrank();
        assertEq(vault.delta(), DELTA + 1);
        assertEq(vault.deltaFloor(), DELTA_FLOOR);

        vm.prank(freezer);
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.raiseDelta(DELTA * 2);
    }

    function _copy(FerrataVault.Checkpoint memory c) internal pure returns (FerrataVault.Checkpoint memory) {
        return FerrataVault.Checkpoint({
            nMin: c.nMin,
            tMin: c.tMin,
            parentState: c.parentState,
            newState: c.newState,
            burnRoot: c.burnRoot,
            burnCount: c.burnCount,
            importedLeafCount: c.importedLeafCount,
            sunset: c.sunset,
            echoNonce: c.echoNonce
        });
    }
}
