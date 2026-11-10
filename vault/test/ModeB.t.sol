// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Pausable} from "openzeppelin/contracts/utils/Pausable.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockRiscZeroVerifier} from "./mocks/MockVerifiers.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Rule (B) (v3 D7, D13): one proof of t final-tier attestations per candidate, final once its wait
///         W = deltaAtSubmit (a reaction window, 1 h here) has passed unless a watcher cancels it. No beacon, no second
///         proof; the finalisation re-presents the candidate's journal, which must match it word for word.
contract ModeBTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("rule B burn root");
    bytes32 internal constant ROOT_2 = keccak256("rule B burn root 2");

    /// The rule (B) vault; the base `vault` is a rule (A) one.
    FerrataVault internal vb;

    function setUp() public override {
        super.setUp();
        vb = new FerrataVault(_deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID));
        _registerDefaultTokens(vb);
        _depositInto(vb, alice, ETH, 1, keccak256("b0"));
        _depositInto(vb, alice, ETH, 1, keccak256("b1"));
    }

    function _next(bytes32 root) internal returns (FerrataVault.Checkpoint memory) {
        return _nextCheckpoint(vb, root, vb.finalImportedLeafCount(), vb.finalSunset());
    }

    function _submitB(FerrataVault.Checkpoint memory c) internal returns (uint256 submittedAt) {
        _submit(vb, LEZ_CONFIG_ID, c);
        return block.timestamp;
    }

    function _copy(FerrataVault.Checkpoint memory c) internal pure returns (FerrataVault.Checkpoint memory d) {
        d = abi.decode(abi.encode(c), (FerrataVault.Checkpoint));
    }

    function test_modeB_constructor_turnSecondsZeroUnderBNonZeroUnderA() public {
        FerrataVault.Deployment memory d = _deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.turnSeconds = 1;
        vm.expectRevert(FerrataVault.InvalidTurnSeconds.selector);
        new FerrataVault(d);
        d = _deployment(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.turnSeconds = 0;
        vm.expectRevert(FerrataVault.InvalidTurnSeconds.selector);
        new FerrataVault(d);
        assertEq(uint8(vb.mode()), uint8(FerrataVault.Mode.B));
        assertEq(uint8(vault.mode()), uint8(FerrataVault.Mode.A));
        assertEq(vb.turnSeconds(), 0);
        assertEq(vb.delta(), DELTA_B);
    }

    function test_modeB_submitRecordsNoBeaconAndKeepsTheJournalDigest() public {
        FerrataVault.Checkpoint memory c = _next(ROOT);
        _proveCheckpoint(_checkpointJournal(vb, LEZ_CONFIG_ID, c));
        vm.expectEmit(true, true, true, true, address(vb));
        emit FerrataVault.CandidateSubmitted(
            c.newState, c.parentState, ROOT, c.burnCount, 0, false, DELTA_B, c.tipId, c.tipHash, N, T
        );
        vb.submitCandidate(c, SEAL);
        uint256 at = block.timestamp;
        assertEq(vb.beaconCount(), 0);
        (bytes32 digest, bytes32 tipHash, uint64 tipId, uint32 t, uint48 submittedAt, uint32 deltaAtSubmit,) =
            vb.candidates(c.newState);
        assertEq(digest, sha256(_checkpointJournalBytes(_checkpointJournal(vb, LEZ_CONFIG_ID, c))));
        assertEq(tipHash, c.tipHash);
        assertEq(tipId, c.tipId);
        assertEq(t, T);
        assertEq(submittedAt, at);
        assertEq(deltaAtSubmit, DELTA_B);
        assertEq(vb.finalState(), GENESIS_STATE); // a candidate accepts nothing
    }

    function test_modeB_finalisesAtItsWaitNotASecondBefore() public {
        FerrataVault.Checkpoint memory c = _next(ROOT);
        c.n = 10;
        c.t = 7; // under (B) the committee does not lengthen the wait
        uint256 at = _submitB(c);
        vm.warp(at + DELTA_B - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vb.finalizeAttested(c);

        vm.warp(at + DELTA_B);
        vm.expectEmit(true, true, true, true, address(vb));
        emit FerrataVault.CheckpointFinalized(
            0, c.newState, c.parentState, ROOT, c.burnCount, 0, false, c.tipId, c.tipHash, 10, 7
        );
        vm.prank(relayer); // permissionless
        vb.finalizeAttested(c);
        assertEq(vb.finalState(), c.newState);
        assertEq(vb.lastFinalizedAt(), block.timestamp);
        (bool accepted, uint256 finalizedAt) = vb.burnRootStatus(ROOT);
        assertTrue(accepted);
        assertEq(finalizedAt, block.timestamp);
        (,,,, uint48 submittedAt,,) = vb.candidates(c.newState);
        assertEq(submittedAt, 0); // consumed
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vb.finalizeAttested(c); // once only
    }

    /// @notice Every word must be the candidate's: the 11 fields the caller can change besides `newState` (the key),
    ///         and `echoNonce`, which under (B) stays 0.
    function test_modeB_everyJournalWordMustBeTheCandidates() public {
        FerrataVault.Checkpoint memory c = _next(ROOT);
        uint256 at = _submitB(c);
        vm.warp(at + DELTA_B);
        for (uint256 field = 0; field < 12; field++) {
            FerrataVault.Checkpoint memory changed = _copy(c);
            if (field == 0) changed.parentState = keccak256("other parent");
            if (field == 1) changed.burnRoot = ROOT_2;
            if (field == 2) changed.burnCount = c.burnCount + 1;
            if (field == 3) changed.importedLeafCount = 1;
            if (field == 4) changed.sunset = true;
            if (field == 5) changed.tipId = c.tipId + 1;
            if (field == 6) changed.tipHash = keccak256("other tip");
            if (field == 7) changed.n = c.n + 1;
            if (field == 8) changed.t = c.t + 1;
            if (field == 9) changed.nMin = N_MIN + 1;
            if (field == 10) changed.tMin = T_MIN + 1;
            if (field == 11) changed.echoNonce = keccak256("a beacon nonce");
            vm.expectRevert(FerrataVault.CandidateMismatch.selector);
            vb.finalizeAttested(changed);
        }
        FerrataVault.Checkpoint memory other = _copy(c);
        other.newState = keccak256("other new state");
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vb.finalizeAttested(other);
        vb.finalizeAttested(c); // control
        assertEq(vb.finalState(), c.newState);
    }

    function test_modeB_beaconsAndTheSecondProofAreRuleAOnly() public {
        vm.expectRevert(FerrataVault.WrongMode.selector);
        vb.recordBeacon();
        FerrataVault.Checkpoint memory c = _next(ROOT);
        uint256 at = _submitB(c);
        vm.warp(at + DELTA_B);
        _proveCheckpoint(_checkpointJournal(vb, LEZ_CONFIG_ID, c));
        vm.expectRevert(FerrataVault.WrongMode.selector);
        vb.finalize(0, c, SEAL);

        FerrataVault.Checkpoint memory a = _nextCheckpoint(ROOT);
        _submit(a);
        vm.warp(block.timestamp + 1 days);
        vm.expectRevert(FerrataVault.WrongMode.selector);
        vault.finalizeAttested(a); // a rule (A) vault never finalises without its second proof
    }

    /// @notice A watcher cancels within the wait (its tip was not already final, D13); the same public proof can be
    ///         submitted again and waits again in full, so cancelling is not sticky (D11).
    function test_modeB_aWatcherCancelsWithinTheWait() public {
        FerrataVault.Checkpoint memory c = _next(ROOT);
        _submitB(c);
        vm.expectEmit(true, true, true, true, address(vb));
        emit FerrataVault.CandidateCancelled(c.newState, c.tipId, c.tipHash);
        vm.prank(watcher);
        vb.cancelCandidate(c.newState);
        vm.warp(block.timestamp + DELTA_B);
        vm.expectRevert(FerrataVault.UnknownCandidate.selector);
        vb.finalizeAttested(c);

        uint256 again = _submitB(c);
        vm.warp(again + DELTA_B - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vb.finalizeAttested(c);
        vm.warp(again + DELTA_B);
        vb.finalizeAttested(c);
    }

    function test_modeB_revocationInvalidatesPendingCandidates() public {
        uint256 first = _acceptRootOn(vb, LEZ_CONFIG_ID, ROOT);
        FerrataVault.Checkpoint memory c = _next(ROOT_2);
        _submitB(c);
        vm.prank(freezer);
        vb.revokeFrom(first);
        vm.warp(block.timestamp + DELTA_B);
        vm.expectRevert(FerrataVault.StaleCandidate.selector);
        vb.finalizeAttested(c);
    }

    function test_modeB_aCompetingCandidateGoesStaleOnceOneIsFinal() public {
        FerrataVault.Checkpoint memory c1 = _next(ROOT);
        FerrataVault.Checkpoint memory c2 = _next(ROOT_2); // same parent: genesis
        _submitB(c1);
        _submitB(c2);
        vm.warp(block.timestamp + DELTA_B);
        vb.finalizeAttested(c1);
        vm.expectRevert(FerrataVault.StaleCandidate.selector);
        vb.finalizeAttested(c2);
    }

    function test_modeB_samePendingStateCannotRestartItsWait() public {
        FerrataVault.Checkpoint memory c = _next(ROOT);
        _submitB(c);
        vm.expectRevert(FerrataVault.CandidateAlreadySubmitted.selector);
        vb.submitCandidate(c, SEAL);
    }

    function test_modeB_checkpointFreezeStopsSubmissionAndFinalisation() public {
        FerrataVault.Checkpoint memory c = _next(ROOT);
        uint256 at = _submitB(c);
        vm.prank(watcher);
        vb.pauseCheckpoints();
        vm.warp(at + DELTA_B);
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vb.finalizeAttested(c);
        FerrataVault.Checkpoint memory c2 = _next(ROOT_2);
        _proveCheckpoint(_checkpointJournal(vb, LEZ_CONFIG_ID, c2));
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vb.submitCandidate(c2, SEAL);
        vm.warp(at + 12 hours); // the watcher's pause has lapsed
        vb.finalizeAttested(c);
    }

    /// @notice Nothing proven is accepted after the verifier stop (D13), under (B) too: a pending candidate past its
    ///         wait is not finalised, and no new one is submitted.
    function test_modeB_nothingIsFinalisedAfterTheStop() public {
        FerrataVault.Checkpoint memory c = _next(ROOT);
        uint256 at = _submitB(c);
        vm.warp(at + DELTA_B);
        vm.prank(freezer);
        estop.estop();
        vm.expectRevert(FerrataVault.VerifierStopped.selector);
        vb.finalizeAttested(c);
        FerrataVault.Checkpoint memory c2 = _next(ROOT_2);
        _proveCheckpoint(_checkpointJournal(vb, LEZ_CONFIG_ID, c2));
        vm.expectRevert(Pausable.EnforcedPause.selector);
        vb.submitCandidate(c2, SEAL);
    }

    function test_modeB_monotonicityAndTheTreeBound() public {
        _fillTreeOf(vb, 6);
        _acceptB(5, false);
        _expectAttestedRevert(4, false, FerrataVault.ImportedLeafCountDecreased.selector);
        _expectAttestedRevert(9, false, FerrataVault.ImportedLeafCountTooHigh.selector); // 8 leaves in the tree
        _acceptB(8, true);
        _expectAttestedRevert(8, false, FerrataVault.SunsetIsFinal.selector);
        _expectAttestedRevert(7, true, FerrataVault.ImportedLeafCountDecreased.selector);
        assertTrue(vb.sunsetLeafCountSet());
        assertEq(vb.sunsetLeafCount(), 8);
    }

    function test_modeB_aDeltaRaiseAppliesToLaterCandidatesOnly() public {
        FerrataVault.Checkpoint memory early = _next(ROOT);
        uint256 at = _submitB(early);
        vm.prank(admin);
        vb.raiseDelta(2 hours);
        FerrataVault.Checkpoint memory late = _next(ROOT_2);
        _submitB(late);
        vm.warp(at + DELTA_B);
        vb.finalizeAttested(early);
        // `late` was built on genesis too; build a successor instead to see its own, longer wait.
        FerrataVault.Checkpoint memory next = _next(ROOT_2);
        uint256 nextAt = _submitB(next);
        vm.warp(nextAt + 2 hours - 1);
        vm.expectRevert(FerrataVault.WaitNotOver.selector);
        vb.finalizeAttested(next);
        vm.warp(nextAt + 2 hours);
        vb.finalizeAttested(next);
    }

    /// @notice The vault pins its rule's program: a proof of the same journal by another image is refused.
    function test_modeB_aProofOfTheOtherRulesProgramIsRefused() public {
        bytes32 ruleBImage = keccak256("test: rule (B) checkpoint program image id");
        FerrataVault.Deployment memory d = _deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.checkpointImageId = ruleBImage;
        FerrataVault other = new FerrataVault(d);
        FerrataVault.Checkpoint memory c = _nextCheckpoint(other, ROOT, 0, false);
        bytes32 digest = sha256(_checkpointJournalBytes(_checkpointJournal(other, LEZ_CONFIG_ID, c)));
        verifier.accept(CHECKPOINT_IMAGE_ID, digest); // rule (A)'s program
        vm.expectRevert(MockRiscZeroVerifier.VerificationFailed.selector);
        other.submitCandidate(c, SEAL);
        verifier.accept(ruleBImage, digest);
        other.submitCandidate(c, SEAL);
    }

    function test_modeB_releaseAgainstARuleBRoot() public {
        _acceptRootOn(vb, LEZ_CONFIG_ID, ROOT);
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, keccak256("nullifier B"));
        ReleaseJournal memory j = ReleaseJournal({
            chainId: block.chainid,
            vault: address(vb),
            lezConfigId: LEZ_CONFIG_ID,
            burnRoot: ROOT,
            nullifier: r.nullifier,
            token: ETH,
            amount: 1 ether,
            recipient: recipient,
            relayer: address(0),
            relayFee: 0
        });
        _proveRelease(j);
        vm.prank(relayer);
        vb.release(r, SEAL);
        assertEq(recipient.balance, 1 ether);
        assertEq(vb.escrowed(ETH), 1 ether);
    }

    // ---- helpers ----------------------------------------------------------------------------------------------------

    function _fillTreeOf(FerrataVault v, uint256 k) internal {
        address filler = makeAddr("rule B tree filler");
        for (uint256 i = 0; i < k; i++) {
            _depositInto(v, filler, ETH, 0, keccak256(abi.encode("rule B filler", i)));
        }
    }

    function _acceptB(uint64 imported, bool sunset) internal {
        FerrataVault.Checkpoint memory c =
            _nextCheckpoint(vb, keccak256(abi.encode("root", imported, sunset)), imported, sunset);
        _submitB(c);
        _finalizeAfterDelta(vb, LEZ_CONFIG_ID, c);
    }

    function _expectAttestedRevert(uint64 imported, bool sunset, bytes4 reason) internal {
        FerrataVault.Checkpoint memory c =
            _nextCheckpoint(vb, keccak256(abi.encode("bad root", imported, sunset)), imported, sunset);
        _submitB(c); // submission does not check monotonicity (v3 D4)
        vm.warp(block.timestamp + DELTA_B);
        vm.expectRevert(reason);
        vb.finalizeAttested(c);
        vm.prank(watcher);
        vb.cancelCandidate(c.newState);
    }
}
