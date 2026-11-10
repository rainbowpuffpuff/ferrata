// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {Ownable} from "openzeppelin/contracts/access/Ownable.sol";
import {Receipt as RiscZeroReceipt} from "risc0/IRiscZeroVerifier.sol";
import {IRiscZeroVerifier as IRiscZeroVerifierFull} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroVerifierEmergencyStop} from "risc0/RiscZeroVerifierEmergencyStop.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockERC20} from "./mocks/MockTokens.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice The roles of v3 D13: watchers (cancel, pause checkpoints for at most 12 h, nothing else), the admin's
///         14-day public proposal that changes the watchers and replaces the freeze authority, keyless retirement
///         after 30 days without a finalisation, the immutable floor under the release buckets, and a verifier that
///         must be RISC Zero's stop (whose owner the deployment renounces).
contract RolesTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("roles burn root");
    uint256 internal constant TIMELOCK = 14 days;
    uint256 internal constant PAUSE = 12 hours;
    uint256 internal constant KEYLESS = 30 days;

    address internal watcher2 = makeAddr("watcher2");
    address internal newFreezer = makeAddr("newFreezer");

    // ---- watchers ---------------------------------------------------------------------------------------------------

    function test_watcher_pausesCheckpointsForTwelveHours() public {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _proveCheckpoint(_checkpointJournal(c));
        uint256 at = block.timestamp;
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.CheckpointsPaused(watcher, at + PAUSE);
        vm.prank(watcher);
        vault.pauseCheckpoints();
        assertEq(vault.checkpointsFrozenUntil(), at + PAUSE);
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(c, SEAL);
        vm.warp(at + PAUSE - 1);
        vm.expectRevert(FerrataVault.CheckpointsAreFrozen.selector);
        vault.submitCandidate(c, SEAL);
        vm.warp(at + PAUSE);
        vault.submitCandidate(c, SEAL);
        // Not other freezes.
        assertFalse(vault.depositsFrozen());
        assertFalse(vault.releasesFrozen());
    }

    function test_watcher_pauseNeverShortensALongerFreeze_andTheFreezeAuthorityCanLiftIt() public {
        vm.prank(freezer);
        vault.setCheckpointsFrozen(true);
        uint256 until = vault.checkpointsFrozenUntil();
        vm.prank(watcher);
        vault.pauseCheckpoints();
        assertEq(vault.checkpointsFrozenUntil(), until);

        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);
        vm.prank(watcher);
        vault.pauseCheckpoints();
        assertTrue(vault.checkpointsFrozen());
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);
        assertFalse(vault.checkpointsFrozen());
    }

    /// @notice A watcher can renew its pause: the risk D13 accepts until the admin replaces it (14 days).
    function test_watcher_renewsItsPauseOnlyFromNow() public {
        vm.prank(watcher);
        vault.pauseCheckpoints();
        vm.warp(block.timestamp + 1 hours);
        vm.prank(watcher);
        vault.pauseCheckpoints();
        assertEq(vault.checkpointsFrozenUntil(), block.timestamp + PAUSE);
    }

    function test_watcher_hasNoOtherPower() public {
        vm.startPrank(watcher);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setCheckpointsFrozen(false);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setReleasesFrozen(true);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setDepositsFrozen(true);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.revokeFrom(0);
        vm.expectRevert(FerrataVault.RetireNotOpen.selector);
        vault.retire();
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.proposeRoles([watcher, watcher2, address(0)], address(0));
        vm.expectRevert(FerrataVault.NotAdmin.selector);
        vault.setGlobalCap(FerrataVault.Direction.Release, 0, 0);
        vm.stopPrank();
    }

    function test_pauseCheckpoints_onlyWatchers() public {
        address[4] memory others = [admin, freezer, alice, address(0xbeef)];
        for (uint256 i = 0; i < others.length; i++) {
            vm.prank(others[i]);
            vm.expectRevert(FerrataVault.NotWatcher.selector);
            vault.pauseCheckpoints();
        }
        assertEq(vault.watchers(0), watcher);
        assertEq(vault.watchers(1), address(0));
        assertEq(vault.watchers(2), address(0));
    }

    // ---- the admin's timelocked role change -------------------------------------------------------------------------

    function test_proposeRoles_onlyTheAdmin() public {
        address[3] memory callers = [freezer, watcher, alice];
        for (uint256 i = 0; i < callers.length; i++) {
            vm.prank(callers[i]);
            vm.expectRevert(FerrataVault.NotAdmin.selector);
            vault.proposeRoles([watcher2, address(0), address(0)], newFreezer);
        }
    }

    function test_applyRoles_fourteenDaysLaterByAnyone() public {
        uint256 at = block.timestamp;
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.RolesProposed([watcher2, alice, address(0)], newFreezer, at + TIMELOCK);
        vm.prank(admin);
        vault.proposeRoles([watcher2, alice, address(0)], newFreezer);
        FerrataVault.RoleProposal memory p = vault.roleProposal();
        assertEq(p.watchers[0], watcher2);
        assertEq(p.watchers[1], alice);
        assertEq(p.freezeAuthority, newFreezer);
        assertEq(p.effectiveAt, at + TIMELOCK);

        vm.warp(at + TIMELOCK - 1);
        vm.expectRevert(FerrataVault.TimelockNotOver.selector);
        vault.applyRoles();
        // Until then the old roles hold.
        vm.prank(watcher);
        vault.pauseCheckpoints();
        vm.prank(freezer);
        vault.setCheckpointsFrozen(false);

        vm.warp(at + TIMELOCK);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.WatchersSet([watcher2, alice, address(0)]);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.FreezeAuthorityTransferred(freezer, newFreezer);
        vm.prank(bob); // permissionless
        vault.applyRoles();

        assertEq(vault.freezeAuthority(), newFreezer);
        assertEq(vault.watchers(0), watcher2);
        assertEq(vault.watchers(1), alice);
        vm.prank(watcher);
        vm.expectRevert(FerrataVault.NotWatcher.selector);
        vault.pauseCheckpoints();
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.setDepositsFrozen(true);
        vm.prank(newFreezer);
        vault.setDepositsFrozen(true);
        vm.prank(alice);
        vault.pauseCheckpoints();

        p = vault.roleProposal();
        assertEq(p.effectiveAt, 0);
        assertEq(p.freezeAuthority, address(0));
        assertEq(p.watchers[0], address(0));
        vm.expectRevert(FerrataVault.NoProposal.selector);
        vault.applyRoles(); // consumed
    }

    function test_applyRoles_nothingProposed() public {
        vm.expectRevert(FerrataVault.NoProposal.selector);
        vault.applyRoles();
    }

    function test_proposeRoles_zeroKeepsTheFreezeAuthority_andAnEmptySetRemovesTheWatchers() public {
        vm.prank(admin);
        vault.proposeRoles([address(0), address(0), address(0)], address(0));
        vm.warp(block.timestamp + TIMELOCK);
        vault.applyRoles();
        assertEq(vault.freezeAuthority(), freezer);
        vm.prank(watcher);
        vm.expectRevert(FerrataVault.NotWatcher.selector);
        vault.pauseCheckpoints();
        // The freeze authority still cancels.
        FerrataVault.Checkpoint memory c = _nextCheckpoint(ROOT);
        _submit(c);
        vm.prank(freezer);
        vault.cancelCandidate(c.newState);
    }

    function test_proposeRoles_aNewProposalRestartsTheClock() public {
        uint256 at = block.timestamp;
        vm.prank(admin);
        vault.proposeRoles([watcher2, address(0), address(0)], newFreezer);
        vm.warp(at + 10 days);
        vm.prank(admin);
        vault.proposeRoles([watcher, address(0), address(0)], address(0)); // withdraws it
        vm.warp(at + TIMELOCK);
        vm.expectRevert(FerrataVault.TimelockNotOver.selector);
        vault.applyRoles();
        vm.warp(at + 10 days + TIMELOCK);
        vault.applyRoles();
        assertEq(vault.freezeAuthority(), freezer);
        assertEq(vault.watchers(0), watcher);
    }

    function test_proposeRoles_theAdminNeverBecomesTheFreezeAuthority() public {
        vm.prank(admin);
        vm.expectRevert(FerrataVault.RolesMustDiffer.selector);
        vault.proposeRoles([watcher, address(0), address(0)], admin);

        // The proposed authority becomes admin meanwhile: applying is refused.
        vm.prank(admin);
        vault.proposeRoles([watcher, address(0), address(0)], newFreezer);
        vm.prank(admin);
        vault.transferAdmin(newFreezer);
        vm.prank(newFreezer);
        vault.acceptAdmin();
        vm.warp(block.timestamp + TIMELOCK);
        vm.expectRevert(FerrataVault.RolesMustDiffer.selector);
        vault.applyRoles();
        assertEq(vault.freezeAuthority(), freezer);
    }

    // ---- keyless retirement -----------------------------------------------------------------------------------------

    function test_retire_anyoneThirtyDaysAfterDeploymentWithoutACheckpoint() public {
        uint256 deployedAt = vault.lastFinalizedAt();
        vm.warp(deployedAt + KEYLESS - 1);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.RetireNotOpen.selector);
        vault.retire();
        vm.warp(deployedAt + KEYLESS);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Retired();
        vm.prank(alice);
        vault.retire();
        assertTrue(vault.retired());
        vm.prank(bob);
        vm.expectRevert(FerrataVault.AlreadyRetired.selector);
        vault.retire();
    }

    function test_retire_aFinalisationRestartsTheClock() public {
        vm.warp(block.timestamp + 20 days);
        _acceptRoot(ROOT);
        uint256 finalizedAt = block.timestamp;
        assertEq(vault.lastFinalizedAt(), finalizedAt);
        vm.warp(finalizedAt + KEYLESS - 1);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.RetireNotOpen.selector);
        vault.retire();
        vm.warp(finalizedAt + KEYLESS);
        vm.prank(alice);
        vault.retire();
        assertTrue(vault.retired());
    }

    function test_retire_theFreezeAuthorityAtAnyTime() public {
        vm.prank(freezer);
        vault.retire();
        assertTrue(vault.retired());
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.AlreadyRetired.selector);
        vault.retire();
    }

    // ---- the floor under the release buckets ------------------------------------------------------------------------

    /// @dev Floor: 1e9 value units of capacity, 1e3 per second. ETH: lezUnit 1e12 wei is worth 3000 value units.
    function _flooredVault() internal returns (FerrataVault v) {
        FerrataVault.Deployment memory d = _deployment(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.releaseCapFloor = _cap(1e9, 1e3);
        v = new FerrataVault(d);
        _registerDefaultTokens(v); // UNLIMITED buckets pass
        assertEq(v.releaseCapFloor(), 1e9);
        assertEq(v.releaseRefillFloor(), 1e3);
    }

    function test_capFloor_theDeploymentsGlobalReleaseBucket() public {
        FerrataVault.Deployment memory d = _deployment(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.releaseCapFloor = _cap(1e9, 1e3);
        d.globalReleaseCap = _cap(1e9 - 1, 1e3);
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        new FerrataVault(d);
        d.globalReleaseCap = _cap(1e9, 1e3 - 1);
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        new FerrataVault(d);
        d.globalReleaseCap = _cap(1e9, 1e3);
        new FerrataVault(d);
    }

    function test_capFloor_globalReleaseBucket() public {
        FerrataVault v = _flooredVault();
        vm.startPrank(admin);
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        v.setGlobalCap(FerrataVault.Direction.Release, 1e9 - 1, 1e3);
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        v.setGlobalCap(FerrataVault.Direction.Release, 1e9, 1e3 - 1);
        v.setGlobalCap(FerrataVault.Direction.Release, 1e9, 1e3);
        v.setGlobalCap(FerrataVault.Direction.Release, UNLIMITED, 0);
        v.setGlobalCap(FerrataVault.Direction.Deposit, 0, 0); // deposits have no floor
        vm.stopPrank();
    }

    function test_capFloor_tokenReleaseBucketInValueUnits() public {
        FerrataVault v = _flooredVault();
        vm.startPrank(admin);
        // 333,333,333,333,333,333 wei is 999,999,999.999 value units: below; one wei more reaches 1e9. Likewise
        // 333,333,333,333 wei per second is 999.999999999 value units per second; one more reaches 1e3.
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        v.setTokenCap(ETH, FerrataVault.Direction.Release, 333_333_333_333_333_333, 333_333_333_334);
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        v.setTokenCap(ETH, FerrataVault.Direction.Release, 333_333_333_333_333_334, 333_333_333_333);
        v.setTokenCap(ETH, FerrataVault.Direction.Release, 333_333_333_333_333_334, 333_333_333_334);
        // A capacity whose value would overflow saturates instead of reverting.
        v.setTokenCap(ETH, FerrataVault.Direction.Release, type(uint256).max - 1, 2 ** 255);
        v.setTokenCap(ETH, FerrataVault.Direction.Deposit, 0, 0); // deposits have no floor
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        v.setTokenCap(address(0xbeef), FerrataVault.Direction.Release, UNLIMITED, 0);

        MockERC20 extra = new MockERC20("Extra", "EXT", 18);
        FerrataVault.TokenParams memory p = _params(_denoms(100e18, 1000e18, 10_000e18), 1e12, 1);
        p.releaseCap = _cap(1e21 - 1e12, 1e15); // 999,999,999 value units
        vm.expectRevert(FerrataVault.CapBelowFloor.selector);
        v.registerToken(address(extra), p);
        p.releaseCap = _cap(1e21, 1e15);
        v.registerToken(address(extra), p);
        vm.stopPrank();
    }

    // ---- the verifier must be a stop that is not tripped ------------------------------------------------------------

    function test_constructor_refusesAVerifierThatIsNotAnUntrippedStop() public {
        vm.expectRevert(FerrataVault.VerifierNotStop.selector);
        new FerrataVault(_deployment(IRiscZeroVerifier(address(verifier)), LEZ_CONFIG_ID)); // the bare base verifier

        RiscZeroVerifierEmergencyStop tripped =
            new RiscZeroVerifierEmergencyStop(IRiscZeroVerifierFull(address(verifier)), freezer);
        vm.prank(freezer);
        tripped.estop();
        vm.expectRevert(FerrataVault.VerifierNotStop.selector);
        new FerrataVault(_deployment(IRiscZeroVerifier(address(tripped)), LEZ_CONFIG_ID));
    }

    /// @notice The deployment renounces the stop's owner (D13): then only RISC Zero's proof of exploit trips it.
    function test_stop_renouncedOwnerCannotStop_aProofOfExploitStill() public {
        vm.prank(freezer);
        estop.renounceOwnership();
        assertEq(estop.owner(), address(0));
        vm.prank(freezer);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, freezer));
        estop.estop();

        verifier.acceptClaim(bytes32(0));
        vm.prank(alice);
        estop.estop(RiscZeroReceipt({seal: SEAL, claimDigest: bytes32(0)}));
        assertTrue(estop.paused());
    }
}
