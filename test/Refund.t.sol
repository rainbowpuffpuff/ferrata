// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §3.4 and §4.5: retirement and refunds of deposits LEZ never imported.
contract RefundTest is VaultTestBase {
    struct Leaf {
        address depositor;
        address token;
        uint8 denomIndex;
        bytes32 commitment;
    }

    Leaf[] internal leafData;
    bytes32[] internal leaves;

    function _dep(address who, address token, uint8 denomIndex, bytes32 commitment) internal returns (uint64 i) {
        i = _deposit(who, token, denomIndex, commitment);
        leafData.push(Leaf(who, token, denomIndex, commitment));
        leaves.push(ReferenceMerkle.leafHash(i, who, token, _amount(token, denomIndex), commitment));
    }

    function _path(uint64 i) internal view returns (bytes32[32] memory p) {
        bytes32[] memory siblings = ReferenceMerkle.path(leaves, i);
        for (uint256 h = 0; h < 32; h++) {
            p[h] = siblings[h];
        }
    }

    /// @dev Finalises a checkpoint with `sunset = true` and the given imported leaf count.
    function _sunset(uint64 importedLeafCount) internal returns (uint256 index) {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vault, keccak256("root at sunset"), importedLeafCount, true);
        _submit(c);
        return _finalizeAfterDelta(c);
    }

    function _refund(uint64 i) internal {
        Leaf memory l = leafData[i];
        vault.refund(i, l.depositor, l.token, l.denomIndex, l.commitment, _path(i));
    }

    /// @dev Builds the path first: it calls the SHA-256 precompile, which vm.expectRevert would count as the call.
    function _expectRefundRevert(uint64 i, bytes4 expectedError) internal {
        Leaf memory l = leafData[i];
        bytes32[32] memory p = _path(i);
        vm.expectRevert(expectedError);
        vault.refund(i, l.depositor, l.token, l.denomIndex, l.commitment, p);
    }

    /// @dev Four deposits (0: alice ETH, 1: bob USDC, 2: bob ETH, 3: alice DAI), then retire.
    function _fourDepositsThenRetire() internal {
        _dep(alice, ETH, 1, keccak256("c0"));
        _dep(bob, address(usdc), 1, keccak256("c1"));
        _dep(bob, ETH, 0, keccak256("c2"));
        _dep(alice, address(dai), 2, keccak256("c3"));
        vm.prank(freezer);
        vault.retire();
    }

    // ---- retire ---------------------------------------------------------------------------------------------------

    function test_retire_isOneWayAndStopsDepositsForever() public {
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Retired();
        vm.prank(freezer);
        vault.retire();
        assertTrue(vault.retired());

        vm.deal(alice, 1 ether);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.depositETH{value: 1 ether}(1, keccak256("c"), 0);

        usdc.mint(alice, 100e6);
        vm.prank(alice);
        usdc.approve(address(vault), 100e6);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.deposit(address(usdc), 0, keccak256("c"), 0);

        vm.prank(freezer);
        vm.expectRevert(FerrataVault.AlreadyRetired.selector);
        vault.retire();
    }

    function test_retire_onlyTheFreezeAuthority() public {
        vm.prank(admin);
        vm.expectRevert(FerrataVault.NotFreezeAuthority.selector);
        vault.retire();
    }

    function test_retire_leavesCheckpointsAndReleasesWorking() public {
        _deposit(alice, ETH, 1, keccak256("c"));
        vm.prank(freezer);
        vault.retire();
        _acceptRoot(keccak256("root"));
        _release(_request(ETH, 1, recipient, keccak256("root"), keccak256("n")));
        assertEq(recipient.balance, 1 ether);
        assertEq(vault.escrowed(ETH), 0);
    }

    // ---- refund ---------------------------------------------------------------------------------------------------

    function test_refund_paysTheDepositorAndReducesEscrow() public {
        _fourDepositsThenRetire();
        _sunset(2); // leaves 0 and 1 were imported on LEZ; 2 and 3 never were

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Refund(2, bob, ETH, 0.1 ether);
        vm.prank(relayer); // permissionless
        _refund(2);
        _refund(3);

        assertEq(bob.balance, 0.1 ether);
        assertEq(dai.balanceOf(alice), 10_000e18);
        assertEq(relayer.balance, 0);
        assertEq(vault.escrowed(ETH), 1 ether);
        assertEq(vault.escrowed(address(dai)), 0);
        assertTrue(vault.refunded(2));
        assertTrue(vault.refunded(3));
        assertFalse(vault.refunded(1));
    }

    function test_refund_impossibleBeforeSunset() public {
        _fourDepositsThenRetire();
        _expectRefundRevert(3, FerrataVault.NotRefundable.selector);

        _acceptRoot(keccak256("an ordinary checkpoint")); // not a sunset
        _expectRefundRevert(3, FerrataVault.NotRefundable.selector);
    }

    function test_refund_impossibleBelowTheSunsetLeafCount() public {
        _fourDepositsThenRetire();
        _sunset(2);
        _expectRefundRevert(1, FerrataVault.NotRefundable.selector); // imported on LEZ: claimable there, not here
        _expectRefundRevert(0, FerrataVault.NotRefundable.selector);
    }

    function test_refund_onlyOnce() public {
        _fourDepositsThenRetire();
        _sunset(0);
        _refund(2); // 0.1 ETH; leaf 0's 1 ETH stays escrowed, so only the refunded flag can stop a second payment
        _expectRefundRevert(2, FerrataVault.AlreadyRefunded.selector);
        assertEq(vault.escrowed(ETH), 1 ether);
    }

    function test_refund_wrongPathReverts() public {
        _fourDepositsThenRetire();
        _sunset(0);
        Leaf memory l = leafData[2];

        bytes32[32] memory good = _path(2);
        bytes32[32] memory otherLeafs = _path(1);
        bytes32[32] memory tampered = _path(2);
        tampered[5] = tampered[5] ^ bytes32(uint256(1));
        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(2, l.depositor, l.token, l.denomIndex, l.commitment, tampered);

        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(2, l.depositor, l.token, l.denomIndex, l.commitment, otherLeafs); // another leaf's path

        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(2, l.depositor, l.token, l.denomIndex + 1, l.commitment, good); // another amount

        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(2, l.depositor, l.token, l.denomIndex, keccak256("other"), good); // another commitment

        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(1, l.depositor, l.token, l.denomIndex, l.commitment, good); // another index
    }

    function test_refund_onlyToTheLeafsDepositor() public {
        _fourDepositsThenRetire();
        _sunset(0);
        Leaf memory l = leafData[2]; // bob's
        bytes32[32] memory p = _path(2);
        vm.prank(relayer);
        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(2, relayer, l.token, l.denomIndex, l.commitment, p); // naming someone else

        vm.prank(relayer);
        vault.refund(2, l.depositor, l.token, l.denomIndex, l.commitment, p);
        assertEq(bob.balance, 0.1 ether); // the depositor is paid, whoever submits
        assertEq(relayer.balance, 0);
    }

    function test_refund_unknownLeafOrToken() public {
        _fourDepositsThenRetire();
        _sunset(0);
        bytes32[32] memory p0 = _path(0);
        vm.expectRevert(FerrataVault.UnknownLeaf.selector);
        vault.refund(4, alice, ETH, 1, keccak256("c"), p0);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.refund(0, alice, address(0xdead), 1, keccak256("c0"), p0);
        vm.expectRevert(FerrataVault.InvalidDenominationIndex.selector);
        vault.refund(0, alice, ETH, 3, keccak256("c0"), p0);
    }

    /// @notice A revocation that removes the sunset checkpoint disables refunds again; a new sunset checkpoint
    ///         re-enables them.
    function test_refund_revokingTheSunsetCheckpointDisablesRefunds() public {
        _fourDepositsThenRetire();
        uint256 index = _sunset(2);
        vm.prank(freezer);
        vault.revokeFrom(index);
        assertFalse(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetLeafCount(), 0);
        _expectRefundRevert(3, FerrataVault.NotRefundable.selector);

        _sunset(2);
        _refund(3);
        assertEq(dai.balanceOf(alice), 10_000e18);
    }

    function test_refund_revokingOnlyLaterCheckpointsKeepsRefunds() public {
        _fourDepositsThenRetire();
        _sunset(2); // checkpoint 0 sets the boundary
        _acceptRoot(keccak256("a later checkpoint")); // checkpoint 1
        vm.prank(freezer);
        vault.revokeFrom(1);
        assertTrue(vault.sunsetLeafCountSet());
        _refund(2);
    }

    function test_refund_aLaterSunsetCheckpointMovesTheBoundary_andItsRevocationClearsIt() public {
        // As specified: every sunset checkpoint sets the boundary, and revoking the one that set it last clears
        // it, even when an earlier sunset checkpoint is still valid (refunds wait for the next sunset checkpoint).
        _fourDepositsThenRetire();
        _sunset(2);
        uint256 second = _sunset(3);
        assertEq(vault.sunsetLeafCount(), 3);
        vm.prank(freezer);
        vault.revokeFrom(second);
        assertFalse(vault.sunsetLeafCountSet());
    }

    function test_refund_escrowRefundedIsNoLongerReleasable() public {
        _dep(alice, ETH, 1, keccak256("c0")); // leaf 0, 1 ETH
        vm.prank(freezer);
        vault.retire();
        _sunset(0);
        _refund(0);
        assertEq(vault.escrowed(ETH), 0);

        _acceptRoot(keccak256("root"));
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, keccak256("root"), keccak256("n"));
        _proveRelease(_releaseJournal(r));
        vm.expectRevert(FerrataVault.InsufficientEscrow.selector);
        vault.release(r, SEAL);
    }

    /// @notice Documents a griefing surface: a refund proves its leaf against the current root, so while deposits
    ///         are still open a new deposit makes an in-flight refund's path stale. Retiring first removes it.
    function test_refund_pathIsAgainstTheCurrentRoot() public {
        _dep(alice, ETH, 1, keccak256("c0"));
        _sunset(0);
        bytes32[32] memory pathNow = _path(0);
        _dep(bob, ETH, 1, keccak256("c1")); // not retired: deposits still change depositRoot
        vm.expectRevert(FerrataVault.InvalidMerklePath.selector);
        vault.refund(0, alice, ETH, 1, keccak256("c0"), pathNow);
        _refund(0); // with a path to the new root
        assertEq(alice.balance, 1 ether);
    }
}
