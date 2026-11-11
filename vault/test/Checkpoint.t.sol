// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §4.3: candidate, beacon, finalisation.
contract CheckpointTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root 1");
    bytes32 internal constant ROOT_2 = keccak256("burn root 2");
    /// The default candidate's wait: delta + (t - 1) turns, with t = 3.
    uint256 internal constant W = DELTA + (uint256(T) - 1) * TURN_SECONDS;

    function _candidate(FerrataVault.Checkpoint memory c)
        internal
        view
        returns (bytes32 digest, uint256 submittedAt, uint256 deltaAtSubmit, uint256 epoch)
    {
        (digest,,,, submittedAt, deltaAtSubmit, epoch,) = vault.candidates(c.newState);
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
        emit FerrataVault.CandidateSubmitted(
            c.newState, GENESIS_STATE, ROOT, c.burnCount, 0, false, DELTA, c.tipId, c.tipHash, N, T
        );
        vm.prank(relayer); // permissionless
        vault.submitCandidate(c, SEAL);

        (bytes32 digest, uint256 submittedAt, uint256 deltaAtSubmit, uint256 epoch) = _candidate(c);
        // v4 stores the digest of the journal the proof was verified against, not the fields.
        assertEq(digest, sha256(_checkpointJournalBytes(_checkpointJournal(c))));
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

    function test_submitCandidate_stoppedByTheCheckpointFreeze() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _proveCheckpoint(_checkpointJournal(c));
        vm.prank(freezer);
        vault.setCheckpointsFrozen(true);
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(c, SEAL);
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);
        vault.submitCandidate(c, SEAL);
    }

    // ---- finalize: the finality rule ---------------------------------------------------------------------------------

    function test_finalize_acceptsTheBurnRoot() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.CheckpointFinalized(
            0, c.newState, GENESIS_STATE, ROOT, c.burnCount, 0, false, c.tipId, c.tipHash, N, T
        );
        vm.prank(relayer); // permissionless
        vault.finalize(b, c, SEAL);

        assertEq(vault.finalState(), c.newState);
        assertEq(vault.revocationEpoch(), 0); // only revocations change it
        assertEq(vault.checkpointCount(), 1);
        _assertFirstCheckpointRecord(c);
        (bool accepted, uint256 at) = vault.burnRootStatus(ROOT);
        assertTrue(accepted);
        assertEq(at, block.timestamp);
        (,, uint256 submittedAt,) = _candidate(c);
        assertEq(submittedAt, 0); // the candidate record is gone
    }

    function _assertFirstCheckpointRecord(FerrataVault.Checkpoint memory c) internal view {
        FerrataVault.FinalizedCheckpoint memory h = _checkpointRecord(vault, 0);
        assertEq(h.parentState, GENESIS_STATE);
        assertEq(h.newState, c.newState);
        assertEq(h.burnRoot, ROOT);
        assertEq(h.burnCount, c.burnCount);
        assertEq(h.importedLeafCount, 0);
        assertFalse(h.sunset);
        assertFalse(h.revoked);
        assertEq(h.parentImportedLeafCount, 0); // genesis carries no imported leaves and no sunset
        assertFalse(h.parentSunset);
        assertEq(h.finalizedAt, block.timestamp);
        // The candidate's sender (this test), not the finalize call's (relayer): credited the checkpoint fee (M-2).
        assertEq(h.prover, address(this));
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
        (uint256 early, bytes32 earlyNonce) = _beaconAt(submittedAt + W - 1);
        c.echoNonce = earlyNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(early, c, SEAL);

        // Exactly delta later is enough.
        (uint256 onTime, bytes32 onTimeNonce) = _beaconAt(submittedAt + W);
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
        (uint256 b1, bytes32 nonce1) = _beaconAt(block.timestamp + W);
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
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;

        // v4: every journal word but echoNonce must be the candidate's (its digest), including the tip, n, t (D11)
        // and the floors; newState is the key, so a changed one names no candidate.
        for (uint256 field = 0; field < 11; field++) {
            FerrataVault.Checkpoint memory changed = _copy(c);
            if (field == 0) changed.parentState = keccak256("other parent");
            if (field == 1) changed.burnRoot = ROOT_2;
            if (field == 2) changed.burnCount = c.burnCount + 1;
            if (field == 3) changed.importedLeafCount = c.importedLeafCount + 1;
            if (field == 4) changed.sunset = true;
            if (field == 5) changed.tipId = c.tipId + 1;
            if (field == 6) changed.tipHash = keccak256("other tip");
            if (field == 7) changed.n = c.n + 1;
            if (field == 8) changed.t = c.t + 1;
            if (field == 9) changed.nMin = N_MIN + 1;
            if (field == 10) changed.tMin = T_MIN - 1;
            _proveCheckpoint(_checkpointJournal(changed)); // a valid proof of the changed statement
            vm.expectRevert(FerrataVault.CandidateMismatch.selector);
            vault.finalize(b, changed, SEAL);
        }
        FerrataVault.Checkpoint memory other = _copy(c);
        other.newState = keccak256("other new state");
        _proveCheckpoint(_checkpointJournal(other));
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vault.finalize(b, other, SEAL);

        _tryFinalize(c, b, nonce);
    }

    function test_finalize_needsItsOwnProof() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c); // the candidate proof (echoNonce = 0) is registered
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
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

        (uint256 beacon, bytes32 nonce) = _beaconAt(block.timestamp + W);
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

        (uint256 b, bytes32 nonce) = _beaconAt(submittedAt + W); // the old delta is enough for this candidate
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

        (uint256 early, bytes32 earlyNonce) = _beaconAt(submittedAt + W);
        c.echoNonce = earlyNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(early, c, SEAL);

        (uint256 late, bytes32 lateNonce) = _beaconAt(submittedAt + DELTA * 3 + (uint256(T) - 1) * TURN_SECONDS);
        _tryFinalize(c, late, lateNonce);
    }

    /// @notice v3 D4: the checkpoint freeze also stops finalisation, and lapses like every freeze.
    function test_finalize_stoppedByTheCheckpointFreezeUntilItLapses() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        vm.prank(freezer);
        vault.setCheckpointsFrozen(true);
        uint256 frozenUntil = vault.checkpointsFrozenUntil();

        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.finalize(b, c, SEAL);

        vm.warp(frozenUntil - 1);
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.finalize(b, c, SEAL);
        vm.warp(frozenUntil); // lapsed, with nobody acting; the same beacon is still old enough
        vault.finalize(b, c, SEAL);
        assertEq(vault.finalState(), c.newState);
    }

    // ---- the committee-sized wait (v3 D4): W = delta + (t - 1) * turnSeconds ---------------------------------------

    function _checkCommitteeSizedWait(uint32 n, uint32 t) internal {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        c.n = n;
        c.t = t;
        _submit(c);
        uint256 submittedAt = block.timestamp;
        (,,, uint32 storedT,,,,) = vault.candidates(c.newState);
        assertEq(storedT, t);
        uint256 wait = DELTA + (uint256(t) - 1) * TURN_SECONDS;

        (uint256 early, bytes32 earlyNonce) = _beaconAt(submittedAt + wait - 1);
        c.echoNonce = earlyNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(early, c, SEAL);

        (uint256 onTime, bytes32 onTimeNonce) = _beaconAt(submittedAt + wait);
        _tryFinalize(c, onTime, onTimeNonce);
        assertEq(vault.finalState(), c.newState);
    }

    function test_finalize_committeeSizedWait_t2() public {
        _checkCommitteeSizedWait(3, 2);
    }

    function test_finalize_committeeSizedWait_t3() public {
        _checkCommitteeSizedWait(4, 3);
    }

    function test_finalize_committeeSizedWait_t5() public {
        _checkCommitteeSizedWait(7, 5);
    }

    function test_finalize_committeeSizedWait_t7() public {
        _checkCommitteeSizedWait(10, 7);
    }

    /// @notice Interpretation: the wait is always the candidate's (D4: "with t from its journal"). The vault does not
    ///         compare tipId, tipHash, n or t between the two journals: `newState` commits to the tip and the key set
    ///         (SPEC-v2 §4.1), so a sound proof of the same state carries the same values. A finalisation journal
    ///         naming another committee only has to meet the floors.
    function test_finalize_theWaitIsTheCandidates() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        c.n = 3;
        c.t = 2;
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + DELTA + TURN_SECONDS);
        // v4 (D11): a finalisation journal with another committee is refused outright, so the wait can only be the
        // candidate's: t = 2 waits one turn, and a beacon after it suffices.
        FerrataVault.Checkpoint memory bigger = _copy(c);
        bigger.n = 10;
        bigger.t = 7;
        bigger.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(bigger));
        vm.expectRevert(FerrataVault.CandidateMismatch.selector);
        vault.finalize(b, bigger, SEAL);
        _tryFinalize(c, b, nonce);
        assertEq(vault.finalState(), c.newState);
    }

    function test_keySetMustMeetTheFloors_atSubmission() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        c.n = N_MIN - 1;
        _proveCheckpoint(_checkpointJournal(c)); // even with a valid proof
        vm.expectRevert(FerrataVault.KeySetBelowFloor.selector);
        vault.submitCandidate(c, SEAL);

        c.n = N;
        c.t = T_MIN - 1;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.KeySetBelowFloor.selector);
        vault.submitCandidate(c, SEAL);
    }

    function test_keySetMustMeetTheFloors_atFinalisation() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;

        // v4: the finalisation journal must be the candidate's word for word, whose key set met the floors at
        // submission, so a finalisation proof's own smaller key set is refused as a mismatch.
        c.n = N_MIN - 1; // the finalisation proof's own key set
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.CandidateMismatch.selector);
        vault.finalize(b, c, SEAL);
        c.n = N;
        c.t = T_MIN - 1;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.CandidateMismatch.selector);
        vault.finalize(b, c, SEAL);

        c.t = T;
        _tryFinalize(c, b, nonce);
    }

    // ---- monotonicity (v3 D4) ----------------------------------------------------------------------------------------

    function _finalizeNext(uint64 importedLeafCount, bool sunset) internal returns (uint256) {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, importedLeafCount, sunset);
        _submit(c);
        return _finalizeAfterDelta(c);
    }

    /// @dev Submits a candidate with these values and expects its finalisation to fail with `expectedError`.
    function _expectFinalizeRevert(uint64 importedLeafCount, bool sunset, bytes4 expectedError) internal {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT_2, importedLeafCount, sunset);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c)); // a valid proof of the offending statement
        vm.expectRevert(expectedError);
        vault.finalize(b, c, SEAL);
    }

    function test_monotonicity_importedLeafCountCannotDecrease() public {
        _fillTree(16); // v4 (D11): imported counts stay within the tree
        _finalizeNext(5, false);
        assertEq(vault.finalImportedLeafCount(), 5);
        _expectFinalizeRevert(4, false, FerrataVault.ImportedLeafCountDecreased.selector);
        _finalizeNext(5, false); // equal is fine
        _finalizeNext(9, false); // and so is more
        assertEq(vault.finalImportedLeafCount(), 9);
    }

    function test_monotonicity_sunsetCannotGoBackToFalse() public {
        _fillTree(16); // v4 (D11): imported counts stay within the tree
        _finalizeNext(5, true);
        assertTrue(vault.finalSunset());
        _expectFinalizeRevert(5, false, FerrataVault.SunsetIsFinal.selector);
        _expectFinalizeRevert(6, true, FerrataVault.SunsetIsFinal.selector); // no imports after the sunset either
        _finalizeNext(5, true);
        assertTrue(vault.finalSunset());
    }

    /// @notice v3 D5: the first sunset checkpoint fixes the boundary; a later one cannot lower it or move it.
    function test_sunsetBoundary_aLaterSunsetCheckpointCannotLowerIt() public {
        _fillTree(16); // v4 (D11): imported counts stay within the tree
        uint256 first = _finalizeNext(5, true);
        uint256 setAt = vault.sunsetFinalizedAt();
        _expectFinalizeRevert(3, true, FerrataVault.ImportedLeafCountDecreased.selector);
        vm.warp(block.timestamp + 1 days);
        _finalizeNext(5, true); // a later sunset checkpoint with the same count changes nothing
        assertEq(vault.sunsetLeafCount(), 5);
        assertEq(vault.sunsetSetAt(), first);
        assertEq(vault.sunsetFinalizedAt(), setAt);
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
        _fillTree(5); // v4 (D11): imported counts stay within the tree
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, 5, true);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.SunsetLeafCountSet(0, 5);
        vault.finalize(b, c, SEAL);
        assertTrue(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetLeafCount(), 5);
        assertEq(vault.sunsetSetAt(), 0);
    }

    // ---- cancelCandidate: reject-only, freeze authority --------------------------------------------------------------

    function test_cancelCandidate_deletesAPendingCandidate() public {
        _acceptRoot(ROOT_2); // a finalised checkpoint, which the cancellation must not touch
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.CandidateCancelled(c.newState, c.tipId, c.tipHash);
        vm.prank(freezer);
        vault.cancelCandidate(c.newState);

        (bytes32 digest, uint256 submittedAt,,) = _candidate(c);
        assertEq(digest, bytes32(0));
        assertEq(submittedAt, 0);
        assertEq(vault.finalState(), c.parentState);
        assertEq(vault.checkpointCount(), 1);
        (bool accepted,) = vault.burnRootStatus(ROOT_2);
        assertTrue(accepted);
        assertEq(vault.revocationEpoch(), 0);
    }

    /// @notice v4 (D13): a watcher or the freeze authority may cancel; nobody else.
    function test_cancelCandidate_onlyAWatcherOrTheFreezeAuthority() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        address[3] memory others = [admin, alice, relayer];
        for (uint256 i = 0; i < others.length; i++) {
            vm.prank(others[i]);
            vm.expectRevert(FerrataVault.NotWatcher.selector);
            vault.cancelCandidate(c.newState);
        }
        (, uint256 submittedAt,,) = _candidate(c);
        assertEq(submittedAt, block.timestamp);

        vm.prank(watcher);
        vault.cancelCandidate(c.newState);
        (, submittedAt,,) = _candidate(c);
        assertEq(submittedAt, 0);
    }

    /// @notice v4 (D11): a checkpoint cannot claim more imported leaves than the tree has; the bound is inclusive.
    function test_finalize_importedLeafCountAboveTheTreeIsRefused() public {
        _fillTree(2);
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, 3, false);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.ImportedLeafCountTooHigh.selector);
        vault.finalize(b, c, SEAL);
        _fillTree(1); // now the tree has 3 leaves
        vault.finalize(b, c, SEAL);
        assertEq(vault.finalImportedLeafCount(), 3);
    }

    function test_cancelCandidate_unknownOrFinalisedCandidateRefused() public {
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vault.cancelCandidate(keccak256("never submitted"));

        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        _finalizeAfterDelta(c);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vault.cancelCandidate(c.newState);
        assertEq(vault.finalState(), c.newState);
        (bool accepted,) = vault.burnRootStatus(ROOT);
        assertTrue(accepted);
    }

    function test_cancelCandidate_aCancelledCandidateCannotBeFinalised() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        (uint256 b, bytes32 nonce) = _beaconAt(block.timestamp + W);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c)); // a valid finalisation proof, and the wait is over
        vm.prank(freezer);
        vault.cancelCandidate(c.newState);

        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vault.finalize(b, c, SEAL);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vault.cancelCandidate(c.newState); // and not twice
        assertEq(vault.finalState(), GENESIS_STATE);
    }

    /// @notice The same state can be proposed again; the new candidate waits its full W from the resubmission.
    function test_cancelCandidate_resubmissionWorksAndRestartsTheWait() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        uint256 firstSubmittedAt = block.timestamp;
        vm.warp(firstSubmittedAt + W - 1 hours);
        vm.prank(freezer);
        vault.cancelCandidate(c.newState);

        vm.roll(block.number + 1);
        _submit(c); // the same statement (echoNonce 0), submitted again
        uint256 resubmittedAt = block.timestamp;
        (, uint256 submittedAt,,) = _candidate(c);
        assertEq(submittedAt, resubmittedAt);

        // Old enough for the first submission, not for the second.
        (uint256 early, bytes32 earlyNonce) = _beaconAt(resubmittedAt + W - 1);
        assertGt(resubmittedAt + W - 1, firstSubmittedAt + W);
        c.echoNonce = earlyNonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.expectRevert(FerrataVault.BeaconTooEarly.selector);
        vault.finalize(early, c, SEAL);

        (uint256 onTime, bytes32 onTimeNonce) = _beaconAt(resubmittedAt + W);
        _tryFinalize(c, onTime, onTimeNonce);
        assertEq(vault.finalState(), c.newState);
    }

    /// @notice Cancelling is reject-only, so neither the checkpoint freeze nor anything else stops it.
    function test_cancelCandidate_worksUnderTheCheckpointFreeze() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        vm.startPrank(freezer);
        vault.setCheckpointsFrozen(true);
        vault.cancelCandidate(c.newState);
        vm.stopPrank();
        (, uint256 submittedAt,,) = _candidate(c);
        assertEq(submittedAt, 0);
    }

    // ---- delta ---------------------------------------------------------------------------------------------------

    function test_raiseDelta_onlyUpAndBelowTheCeiling() public {
        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.DeltaCanOnlyRise.selector);
        vault.raiseDelta(DELTA - 1);
        uint256 ceiling = MAX_DELTA_;
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
            echoNonce: c.echoNonce,
            tipId: c.tipId,
            tipHash: c.tipHash,
            n: c.n,
            t: c.t
        });
    }
}
