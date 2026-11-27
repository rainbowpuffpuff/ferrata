// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {StdInvariant} from "forge-std/StdInvariant.sol";
import {console2} from "forge-std/console2.sol";

import {FerrataVault} from "../../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../../src/interfaces/IRiscZeroVerifier.sol";
import {MockTokenBase} from "../mocks/MockTokens.sol";
import {ReferenceMerkle} from "../utils/ReferenceMerkle.sol";
import {VaultTestBase} from "../utils/VaultTestBase.sol";
import {EntitlementHandler} from "./EntitlementHandler.sol";

/// forge-config: default.invariant.runs = 128
/// forge-config: default.invariant.depth = 64
/// @notice RFP-022 Reliability #4 on the vault: later activity can delay, but must never invalidate, an accepted burn, an
///         owed balance, or a refundable leaf. Written by an e-INFRA worker (Kimi K3.1 Pro) for the internal review of
///         27 November 2026, read line by line before it was brought in; two mutations of `_checkRelease` (refusing a
///         delisted token; accepting only the newest checkpoint's root) each make it fail.
/// @dev Disclosed exceptions, deliberately outside the handler: tripping RISC Zero's emergency stop, `revokeFrom` of
///      the checkpoint that accepted an entitlement's root, and a verifier that accepts forged proofs. The handler
///      never calls `revokeFrom`. Honest novel checkpoints, not redeliveries of an old root, provide later finality.
contract EntitlementInvariants is VaultTestBase {
    EntitlementHandler internal handler;

    uint256 internal ghostReleaseFailures;
    uint256 internal ghostClaimFailures;
    uint256 internal ghostRefundFailures;

    function setUp() public override {
        super.setUp();

        // Rule B makes each honest checkpoint independent of beacon scheduling. A nonzero release floor still leaves
        // enough capacity for every tracked entitlement after the bucket settles.
        FerrataVault.Deployment memory d = _deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID);
        d.minReleaseDelay = 1 hours;
        d.releaseCapFloor = _cap(101 ether, 0.001 ether);
        vault = new FerrataVault(d);
        vm.startPrank(admin);
        FerrataVault.TokenParams memory ethParams = _params(_denoms(0.1 ether, 1 ether, 10 ether), 1e12, 1e12);
        ethParams.releaseDelay = d.minReleaseDelay;
        vault.registerToken(ETH, ethParams);
        FerrataVault.TokenParams memory daiParams = _params(_denoms(100e18, 1000e18, 10_000e18), 1e12, 1);
        daiParams.releaseDelay = d.minReleaseDelay;
        vault.registerToken(address(dai), daiParams);
        vm.stopPrank();

        handler = new EntitlementHandler(vault, verifier, address(dai), admin);
        handler.initialize(); // accepted burns, an owed payout and refund candidates exist from the first fuzz step

        bytes4[] memory selectors = new bytes4[](21);
        selectors[0] = EntitlementHandler.deposit.selector;
        selectors[1] = EntitlementHandler.submitCandidate.selector;
        selectors[2] = EntitlementHandler.finalizeCandidate.selector;
        selectors[3] = EntitlementHandler.cancelCandidate.selector;
        selectors[4] = EntitlementHandler.pauseCheckpoints.selector;
        selectors[5] = EntitlementHandler.retire.selector;
        selectors[6] = EntitlementHandler.createRelease.selector;
        selectors[7] = EntitlementHandler.releaseOutstanding.selector;
        selectors[8] = EntitlementHandler.claimOutstanding.selector;
        selectors[9] = EntitlementHandler.refundOutstanding.selector;
        selectors[10] = EntitlementHandler.warp.selector;
        selectors[11] = EntitlementHandler.setTokenCap.selector;
        selectors[12] = EntitlementHandler.setGlobalCap.selector;
        selectors[13] = EntitlementHandler.setDepositsEnabled.selector;
        selectors[14] = EntitlementHandler.setReleaseDelay.selector;
        selectors[15] = EntitlementHandler.raiseDelta.selector;
        selectors[16] = EntitlementHandler.announceFees.selector;
        selectors[17] = EntitlementHandler.applyFees.selector;
        selectors[18] = EntitlementHandler.proposeAndApplyRoles.selector;
        selectors[19] = EntitlementHandler.transferAndAcceptAdmin.selector;
        selectors[20] = EntitlementHandler.setFreeze.selector;
        targetContract(address(handler));
        targetSelector(StdInvariant.FuzzSelector({addr: address(handler), selectors: selectors}));
    }

    /// @dev Each check performs the real payout on a fresh snapshot and restores every vault, token, verifier and
    ///      mock-recipient effect. Only the failure counters survive the restore.
    function invariant_acceptedBurnReleasesAfterLaterActivity() public {
        uint256 count = handler.releaseCount();
        for (uint256 i = 0; i < count; i++) {
            uint256 snap = vm.snapshotState();
            (bool good, bytes memory reason) = _checkRelease(i);
            assertTrue(vm.revertToState(snap), "release snapshot restore");
            if (!good) _recordFailure(0, reason);
        }
        assertEq(ghostReleaseFailures, 0, "outstanding release became invalid or paid incorrectly");
    }

    function _checkRelease(uint256 i) internal returns (bool good, bytes memory reason) {
        FerrataVault.ReleaseRequest memory r = handler.releaseAtView(i);
        _settle(ETH);
        vault.applyFees(r.token);
        (uint64 current,) = vault.feeState(r.token);
        (,, uint256 burnFee, uint256 checkpointFee) = vault.fees(r.token, current, r.denomIndex);
        uint256 amount = _amount(r.token, r.denomIndex);
        _proveRelease(_releaseJournal(r));

        address feePayee = r.relayer == address(0) ? handler.relayer() : r.relayer;
        uint256 recipientBefore = _entitlementValue(r.token, r.recipient);
        uint256 relayerBefore = _entitlementValue(r.token, feePayee);
        vm.prank(handler.relayer());
        bool ok;
        (ok, reason) = address(vault).call(abi.encodeCall(FerrataVault.release, (r, SEAL)));
        uint256 recipientGain = _entitlementValue(r.token, r.recipient) - recipientBefore;
        uint256 relayerGain = _entitlementValue(r.token, feePayee) - relayerBefore;
        good = ok && vault.nullifierUsed(r.nullifier)
            && recipientGain == amount - r.relayFee - burnFee - checkpointFee && relayerGain == r.relayFee;
    }

    function invariant_owedBalanceCanBeClaimedAfterLaterActivity() public {
        uint256 count = handler.owedKeyCount();
        for (uint256 i = 0; i < count; i++) {
            (address payee, address token) = handler.owedKeyAt(i);
            uint256 amount = vault.owed(payee, token);
            if (amount == 0) continue;
            uint256 snap = vm.snapshotState();
            if (payee == address(handler.toggle())) handler.toggle().setOpen(true);
            uint256 before = _balance(token, payee);
            vm.prank(payee);
            (bool ok, bytes memory reason) = address(vault).call(abi.encodeCall(FerrataVault.claimOwed, (token)));
            bool good = ok && vault.owed(payee, token) == 0 && _balance(token, payee) - before == amount;
            assertTrue(vm.revertToState(snap), "claim snapshot restore");
            if (!good) _recordFailure(1, reason);
        }
        assertEq(ghostClaimFailures, 0, "outstanding owed balance became unclaimable or paid incorrectly");
    }

    function invariant_unimportedLeafRefundsAfterLaterActivity() public {
        if (!vault.retired() || !vault.sunsetLeafCountSet()) return;
        uint256 count = handler.refundCandidateCount();
        for (uint256 i = 0; i < count; i++) {
            EntitlementHandler.Leaf memory leaf = handler.refundCandidateAt(i);
            if (leaf.index < vault.sunsetLeafCount() || vault.refunded(leaf.index)) continue;
            uint256 snap = vm.snapshotState();
            _settle(leaf.token);
            bytes32[] memory siblings = ReferenceMerkle.path(handler.leaves(), leaf.index);
            bytes32[32] memory path;
            for (uint256 h = 0; h < 32; h++) {
                path[h] = siblings[h];
            }
            uint256 amount = _amount(leaf.token, leaf.denomIndex) + _relayFee(leaf.token, leaf.denomIndex);
            uint256 before = _entitlementValue(leaf.token, leaf.depositor);
            vm.prank(handler.relayer());
            (bool ok, bytes memory reason) = address(vault).call(
                abi.encodeCall(
                    FerrataVault.refund,
                    (leaf.index, leaf.depositor, leaf.token, leaf.denomIndex, leaf.commitment, path)
                )
            );
            bool good = ok && vault.refunded(leaf.index)
                && _entitlementValue(leaf.token, leaf.depositor) - before == amount;
            assertTrue(vm.revertToState(snap), "refund snapshot restore");
            if (!good) _recordFailure(2, reason);
        }
        assertEq(ghostRefundFailures, 0, "outstanding refund became invalid or paid incorrectly");
    }

    function invariant_noOutstandingEntitlementWasLost() public view {
        assertEq(ghostReleaseFailures, 0, "outstanding release became invalid or paid incorrectly");
        assertEq(ghostClaimFailures, 0, "outstanding owed balance became unclaimable or paid incorrectly");
        assertEq(ghostRefundFailures, 0, "outstanding refund became invalid or paid incorrectly");
    }

    /// @dev Low-level calls keep the campaign alive long enough to count every failed entitlement check.
    function _recordFailure(uint8 kind, bytes memory reason) internal {
        console2.log("entitlement check failed (0 release, 1 claim, 2 refund)", kind);
        if (reason.length != 0) console2.logBytes(reason);
        if (kind == 0) ghostReleaseFailures++;
        else if (kind == 1) ghostClaimFailures++;
        else ghostRefundFailures++;
    }

    function _settle(address token) internal {
        uint256 wait = vault.maxFreeze() + vault.revokeWindow() + 1;
        (uint256 capacity, uint256 refill,) = vault.tokenCap(token, FerrataVault.Direction.Release);
        if (capacity != UNLIMITED && refill != 0) wait = _max(wait, capacity / refill + 1);
        (uint256 globalCapacity, uint256 globalRefill,) = vault.globalCap(FerrataVault.Direction.Release);
        if (globalCapacity != UNLIMITED && globalRefill != 0) {
            wait = _max(wait, globalCapacity / globalRefill + 1);
        }
        vm.warp(block.timestamp + wait);
    }

    function _max(uint256 a, uint256 b) internal pure returns (uint256) {
        return a > b ? a : b;
    }

    function _entitlementValue(address token, address who) internal view returns (uint256) {
        return _balance(token, who) + vault.owed(who, token);
    }

    // -------------------------------------------------------------------------------------------------------------
    // Plain cases: each creates its own burn, performs later activity, then verifies the original proof still pays.
    // -------------------------------------------------------------------------------------------------------------

    function _pendingRelease(bytes32 salt) internal returns (FerrataVault.ReleaseRequest memory r) {
        _deposit(alice, ETH, 1, keccak256(abi.encode("unit deposit", salt)));
        bytes32 root = keccak256(abi.encode("unit root", salt));
        _acceptRoot(root);
        r = _request(ETH, 1, recipient, root, keccak256(abi.encode("unit nullifier", salt)));
    }

    function _releaseAfterUnitWait(FerrataVault.ReleaseRequest memory r) internal {
        vm.warp(block.timestamp + vault.maxFreeze() + vault.revokeWindow() + 200 days);
        _release(r);
        assertEq(recipient.balance, 1 ether);
    }

    function test_pendingRelease_survivesRenewedReleaseFreezeThenLapse() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("freeze");
        vm.prank(freezer);
        vault.setReleasesFrozen(true);
        vm.warp(block.timestamp + MAX_FREEZE - 1 hours);
        vm.prank(freezer);
        vault.setReleasesFrozen(true); // renewed once, then nobody lifts or renews it
        _releaseAfterUnitWait(r);
    }

    function test_pendingRelease_survivesTokenReleaseCapLoweredToFloor() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("cap floor");
        vm.startPrank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 101 ether, 0.001 ether);
        vault.setGlobalCap(FerrataVault.Direction.Release, 101 ether, 0.001 ether);
        vm.stopPrank();
        _releaseAfterUnitWait(r);
    }

    function test_pendingRelease_survivesTokenDelisting() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("delisted");
        vm.prank(admin);
        vault.setDepositsEnabled(ETH, false);
        _releaseAfterUnitWait(r);
    }

    function test_pendingRelease_survivesOnePercentFeeCeilingSchedule() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("fees");
        uint256[] memory denoms = _denoms(0.1 ether, 1 ether, 10 ether);
        uint256[] memory mint = new uint256[](3);
        uint256[] memory burn = new uint256[](3);
        uint256[] memory checkpoint = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            mint[i] = denoms[i] / 100;
            burn[i] = denoms[i] / 100;
            checkpoint[i] = denoms[i] / 100;
        }
        vm.prank(admin);
        vault.announceFees(ETH, mint, burn, checkpoint);
        vm.warp(block.timestamp + FEE_NOTICE);
        vault.applyFees(ETH);
        vm.warp(block.timestamp + vault.maxFreeze() + vault.revokeWindow() + 1);
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        vault.release(r, SEAL);
        // independently: 1% burn + 1% checkpoint fee off the denomination (where the burn fee goes is the fee code's,
        // ADR 0046 or 0048, and not this test's)
        assertEq(recipient.balance, 0.98 ether);
    }

    function test_pendingRelease_survivesDeltaRaisedToThirtyDays() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("delta");
        vm.prank(admin);
        vault.raiseDelta(30 days);
        assertEq(vault.delta(), 30 days);
        _releaseAfterUnitWait(r);
    }

    function test_pendingRelease_survivesWatcherAndFreezerTimelockReplacement() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("roles");
        address newWatcher = makeAddr("new watcher");
        address newFreezer = makeAddr("new freezer");
        vm.prank(admin);
        vault.proposeRoles([newWatcher, address(0), address(0)], newFreezer);
        vm.warp(block.timestamp + 14 days);
        vault.applyRoles();
        assertEq(vault.watchers(0), newWatcher);
        assertEq(vault.freezeAuthority(), newFreezer);
        vm.prank(newWatcher);
        vault.pauseCheckpoints();
        _releaseAfterUnitWait(r);
    }

    function test_pendingRelease_survivesVaultRetirement() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("retired");
        vm.prank(freezer);
        vault.retire();
        assertTrue(vault.retired());
        _releaseAfterUnitWait(r);
    }

    function test_pendingRelease_survivesLaterCheckpointWithDifferentBurnRoot() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("first root");
        bytes32 oldRoot = r.burnRoot;
        _acceptRoot(keccak256("independently finalised second burn root"));
        (bool accepted,) = vault.burnRootStatus(oldRoot);
        assertTrue(accepted);
        assertEq(vault.checkpointCount(), 3); // seeded root, this unit's root, then the independent later root
        _releaseAfterUnitWait(r);
    }

    function test_retiredSunsetCheckpointMakesTrackedLeafRefundableAfterDelay() public {
        EntitlementHandler.Leaf memory leaf = handler.refundCandidateAt(0);
        uint256 before = _balance(leaf.token, leaf.depositor);
        handler.retire();
        handler.submitCandidate(0);
        handler.finalizeCandidate(0);
        assertTrue(vault.sunsetLeafCountSet());
        handler.refundOutstanding(0);
        assertTrue(vault.refunded(leaf.index));
        assertEq(
            _balance(leaf.token, leaf.depositor) - before,
            _amount(leaf.token, leaf.denomIndex) + _relayFee(leaf.token, leaf.denomIndex)
        );
    }

    function test_disclosedException_revokingRootCheckpointInvalidatesPendingRelease() public {
        FerrataVault.ReleaseRequest memory r = _pendingRelease("revocation exception");
        uint256 rootCheckpoint = vault.burnRootCheckpoint(r.burnRoot) - 1;
        vm.prank(freezer);
        vault.revokeFrom(rootCheckpoint);
        _proveRelease(_releaseJournal(r));
        vm.expectRevert(FerrataVault.BurnRootNotAccepted.selector);
        vault.release(r, SEAL);
        assertFalse(vault.nullifierUsed(r.nullifier));
        assertGt(_balance(ETH, address(vault)), 0);
    }

    function afterInvariant() public view {
        (uint256 created, uint256 released, uint256 claims, uint256 refunds) = handler.summary();
        console2.log("entitlements created", created);
        console2.log("other releases collected", released);
        console2.log("owed claims collected", claims);
        console2.log("refunds collected", refunds);
        console2.log("release / claimed / refunds still tracked", handler.releaseCount());
        console2.log("refund candidates still tracked", handler.refundCandidateCount());
    }
}
