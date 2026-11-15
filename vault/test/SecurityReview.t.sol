// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {DeployVault} from "../script/DeployVault.s.sol";
import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Regressions for the vault findings of the security review of 11 November 2026
///         (notes/reviews/security/review-2026-11-11.md, integration4 at bc4a2a6). Each test fails on that vault and
///         passes on the fixed one; they use only the interface both share, so the same file runs against either.
contract SecurityReviewTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("security review root");
    uint256 internal constant KEYLESS = 30 days;
    /// The shortest candidate wait of the default deployment: delta + (tMin - 1) turns.
    uint256 internal constant MIN_WAIT = DELTA + (uint256(T_MIN) - 1) * TURN_SECONDS;
    uint256 internal constant PAUSE = 12 hours;

    address internal prover = makeAddr("prover");

    function _retireRefused() internal {
        vm.prank(alice);
        vm.expectRevert(FerrataVault.RetireNotOpen.selector);
        vault.retire();
    }

    function _retireByAnyone() internal {
        vm.prank(alice);
        vault.retire();
        assertTrue(vault.retired());
    }

    // ---- M-1: keyless retirement opens on inactivity ------------------------------------------------------------

    /// @notice The clock starts at the first moment a checkpoint can be finalised (one shortest wait after
    ///         deployment), not at deployment.
    function test_M1_keylessRetirementCountsFromTheFirstFinalisableMoment() public {
        uint256 t0 = block.timestamp;
        vm.warp(t0 + KEYLESS); // the opening at bc4a2a6
        _retireRefused();
        vm.warp(t0 + MIN_WAIT + KEYLESS - 1);
        _retireRefused();
        vm.warp(t0 + MIN_WAIT + KEYLESS);
        _retireByAnyone();
    }

    /// @notice A checkpoint freeze stops the clock, also when the freeze authority lifts it early.
    function test_M1_aCheckpointFreezeLiftedEarlyStillStopsTheClock() public {
        uint256 t0 = block.timestamp;
        vm.warp(t0 + 1 days);
        vm.prank(freezer);
        vault.setCheckpointsFrozen(true);
        vm.warp(t0 + 29 days);
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);
        vm.warp(t0 + KEYLESS + MIN_WAIT);
        _retireRefused();
        uint256 opens = t0 + 29 days + MIN_WAIT + KEYLESS;
        vm.warp(opens - 1);
        _retireRefused();
        vm.warp(opens);
        _retireByAnyone();
    }

    /// @notice A checkpoint freeze that runs its full `maxFreeze` (30 days) does not let the clock run out under it.
    function test_M1_aFullCheckpointFreezeStopsTheClock() public {
        uint256 t0 = block.timestamp;
        vm.warp(t0 + 1 days);
        vm.prank(freezer);
        vault.setCheckpointsFrozen(true);
        uint256 lapses = t0 + 1 days + MAX_FREEZE;
        vm.warp(lapses);
        _retireRefused();
        vm.warp(lapses + MIN_WAIT + KEYLESS - 1);
        _retireRefused();
        vm.warp(lapses + MIN_WAIT + KEYLESS);
        _retireByAnyone();
    }

    /// @notice One watcher renewing its 12-hour pause cannot run the clock out.
    function test_M1_oneWatcherPausingCannotCauseIt() public {
        uint256 t0 = block.timestamp;
        for (uint256 i = 0; i <= 80; i++) {
            vm.warp(t0 + i * PAUSE);
            vm.prank(watcher);
            vault.pauseCheckpoints();
        }
        // Day 40, no finalisation since deployment.
        _retireRefused();
        uint256 opens = t0 + 80 * PAUSE + PAUSE + MIN_WAIT + KEYLESS;
        vm.warp(opens - 1);
        _retireRefused();
        vm.warp(opens);
        _retireByAnyone();
    }

    /// @notice One watcher cancelling every candidate cannot run the clock out while someone resubmits: each verified
    ///         submission restarts it. Once nobody submits, it opens (abandonment).
    function test_M1_oneWatcherCancellingCannotCauseIt() public {
        uint256 t0 = block.timestamp;
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _proveCheckpoint(_checkpointJournal(c));
        for (uint256 day = 0; day <= 40; day += 5) {
            vm.warp(t0 + day * 1 days);
            vm.prank(prover);
            vault.submitCandidate(c, SEAL);
            vm.prank(watcher);
            vault.cancelCandidate(c.newState);
        }
        _retireRefused();
        uint256 opens = t0 + 40 days + MIN_WAIT + KEYLESS;
        vm.warp(opens - 1);
        _retireRefused();
        vm.warp(opens);
        _retireByAnyone();
    }

    // ---- M-2: the checkpoint fee goes to the prover -------------------------------------------------------------

    /// @dev Announces a 0.5% checkpoint fee on ETH's 1-ether denomination of `v` and waits out its notice.
    function _checkpointFeeOn(FerrataVault v) internal returns (uint256 fee) {
        fee = 1 ether / 200;
        uint256[] memory zero = new uint256[](3);
        uint256[] memory ckpt = new uint256[](3);
        ckpt[1] = fee;
        vm.prank(admin);
        v.announceFees(ETH, zero, zero, ckpt);
        vm.warp(block.timestamp + 7 days);
    }

    function _releaseOn(FerrataVault v, bytes32 nullifier) internal {
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, ROOT, nullifier);
        ReleaseJournal memory j = _releaseJournal(r);
        j.vault = address(v);
        _proveRelease(j);
        vm.warp(block.timestamp + REVOKE_WINDOW + 1);
        vm.prank(relayer);
        v.release(r, SEAL);
    }

    /// @notice Rule (B): finalizeAttested needs no proof, so its sender gets nothing; the candidate's prover is paid.
    function test_M2_ruleB_theCheckpointFeeGoesToTheProverNotToTheFinaliser() public {
        FerrataVault vb = new FerrataVault(_deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID));
        _registerDefaultTokens(vb);
        uint256 fee = _checkpointFeeOn(vb);
        _depositInto(vb, alice, ETH, 1, keccak256("m2 rule B"));
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vb, ROOT, 0, false);
        _proveCheckpoint(_checkpointJournal(vb, LEZ_CONFIG_ID, c));
        vm.prank(prover);
        vb.submitCandidate(c, SEAL);
        (uint256 submittedAt, uint256 wait) = _waitOf(vb, c.newState);
        vm.warp(submittedAt + wait);
        vm.prank(bob); // rebuilt the journal from CandidateSubmitted; no proof
        vb.finalizeAttested(c);
        _releaseOn(vb, keccak256("m2 rule B nullifier"));
        assertEq(vb.owed(prover, ETH), fee);
        assertEq(vb.owed(bob, ETH), 0);
    }

    /// @notice Rule (A): copying a pending finalisation (its seal is not bound to the sender) earns nothing.
    function test_M2_ruleA_aCopiedFinalisationEarnsNothing() public {
        uint256 fee = _checkpointFeeOn(vault);
        _deposit(alice, ETH, 1, keccak256("m2 rule A"));
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _proveCheckpoint(_checkpointJournal(c));
        vm.prank(prover);
        vault.submitCandidate(c, SEAL);
        (uint256 submittedAt, uint256 wait) = _waitOf(vault, c.newState);
        vm.warp(submittedAt + wait);
        vm.roll(block.number + 1);
        uint256 beaconIndex = vault.recordBeacon();
        (, bytes32 nonce) = vault.beacons(beaconIndex);
        c.echoNonce = nonce;
        _proveCheckpoint(_checkpointJournal(c));
        vm.prank(bob); // sent first with the prover's seal
        vault.finalize(beaconIndex, c, SEAL);
        _releaseOn(vault, keccak256("m2 rule A nullifier"));
        assertEq(vault.owed(prover, ETH), fee);
        assertEq(vault.owed(bob, ETH), 0);
    }

    // ---- L-1: stale candidates ----------------------------------------------------------------------------------

    /// @notice X's candidate P -> N goes stale when Y's P -> N' is finalised; N' -> N can then be submitted.
    function test_L1_aStaleCandidateNoLongerBlocksItsState() public {
        FerrataVault.Checkpoint memory x = _nextCheckpoint(ROOT); // P -> N
        FerrataVault.Checkpoint memory y = _nextCheckpoint(keccak256("other root")); // P -> N'
        _submit(x);
        _submit(y);
        _finalizeAfterDelta(y);
        FerrataVault.Checkpoint memory z = x; // N' -> N: same state, built on the new finalState
        z.parentState = y.newState;
        z.echoNonce = bytes32(0);
        z.burnCount = y.burnCount + 1;
        _submit(z);
        _finalizeAfterDelta(z);
        assertEq(vault.finalState(), x.newState);
    }

    // ---- L-3: deposits after a finalised sunset checkpoint ------------------------------------------------------

    function _finalizeSunset() internal returns (uint256 finalizedAt) {
        _deposit(alice, ETH, 0, keccak256("imported"));
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, ROOT, 1, true);
        _submit(c);
        _finalizeAfterDelta(c);
        assertTrue(vault.finalSunset());
        finalizedAt = block.timestamp;
    }

    function test_L3_noDepositAfterAFinalisedSunsetCheckpoint() public {
        _finalizeSunset();
        uint256 total = _depositTotal(vault, ETH, 0);
        vm.deal(bob, total);
        vm.prank(bob);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.depositETH{value: total}(0, keccak256("after sunset"), 0);
    }

    /// @notice Anyone may retire once the sunset checkpoint can no longer be revoked (the second refunds open).
    function test_L3_anyoneRetiresOnceTheSunsetCheckpointIsIrrevocable() public {
        uint256 finalizedAt = _finalizeSunset();
        vm.warp(finalizedAt + REVOKE_WINDOW);
        _retireRefused();
        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        _retireByAnyone();
    }

    // ---- I-7: the bound of revokeFrom's loop --------------------------------------------------------------------

    function test_I7_theConstructorBoundsRevokeWindowOverDeltaFloor() public {
        FerrataVault.Deployment memory d = _deployment(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.revokeWindow = DELTA_FLOOR * 1001;
        vm.expectRevert(FerrataVault.InvalidRevokeWindow.selector);
        new FerrataVault(d);
        d.revokeWindow = DELTA_FLOOR * 1001 - 1; // revokeWindow / deltaFloor = 1000
        new FerrataVault(d);
    }

    // ---- I-2: the deployment script ---------------------------------------------------------------------------

    function test_I2_theTestDeploymentRefusesEveryMainnet() public {
        uint256[3] memory mainnets = [uint256(1), 10, 8453];
        for (uint256 i = 0; i < mainnets.length; i++) {
            vm.chainId(mainnets[i]);
            DeployVault s = new DeployVault();
            vm.expectRevert(bytes("DeployVault: a test deployment: anvil, Sepolia, Hoodi or Holesky only"));
            s.run();
        }
    }
}
