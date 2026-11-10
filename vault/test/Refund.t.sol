// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Spec §3.4 and §4.5, with v3 D5: retirement and refunds of deposits LEZ never imported. Refunds need a
///         retired vault, a sunset checkpoint at least `revokeWindow` old and no global release freeze.
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
        leaves.push(_leafHash(i, who, token, _amount(token, denomIndex), commitment));
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

    /// @dev Waits until refunds open: the second after the sunset checkpoint's last revocable one (v3 D5, D11).
    function _openRefunds() internal {
        uint256 opensAt = vault.sunsetFinalizedAt() + REVOKE_WINDOW + 1;
        if (block.timestamp < opensAt) vm.warp(opensAt);
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

    /// @notice v4 (D13): others may retire only after KEYLESS_RETIREMENT without a finalisation (test/Roles.t.sol).
    function test_retire_othersOnlyAfterThirtyDaysWithoutACheckpoint() public {
        vm.prank(admin);
        vm.expectRevert(FerrataVault.RetireNotOpen.selector);
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
        _openRefunds();

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
        _openRefunds();
        _expectRefundRevert(1, FerrataVault.NotRefundable.selector); // imported on LEZ: claimable there, not here
        _expectRefundRevert(0, FerrataVault.NotRefundable.selector);
    }

    function test_refund_onlyOnce() public {
        _fourDepositsThenRetire();
        _sunset(0);
        _openRefunds();
        _refund(2); // 0.1 ETH; leaf 0's 1 ETH stays escrowed, so only the refunded flag can stop a second payment
        _expectRefundRevert(2, FerrataVault.AlreadyRefunded.selector);
        assertEq(vault.escrowed(ETH), 1 ether);
    }

    function test_refund_wrongPathReverts() public {
        _fourDepositsThenRetire();
        _sunset(0);
        _openRefunds();
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
        _openRefunds();
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
        _openRefunds();
        bytes32[32] memory p0 = _path(0);
        vm.expectRevert(FerrataVault.UnknownLeaf.selector);
        vault.refund(4, alice, ETH, 1, keccak256("c"), p0);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.refund(0, alice, address(0xdead), 1, keccak256("c0"), p0);
        vm.expectRevert(FerrataVault.InvalidDenominationIndex.selector);
        vault.refund(0, alice, ETH, 3, keccak256("c0"), p0);
    }

    /// @notice A revocation that removes the sunset checkpoint disables refunds again; a new sunset checkpoint
    ///         re-enables them, a full `revokeWindow` after its own finalisation.
    function test_refund_revokingTheSunsetCheckpointDisablesRefunds() public {
        _fourDepositsThenRetire();
        uint256 index = _sunset(2);
        vm.prank(freezer);
        vault.revokeFrom(index);
        assertFalse(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetLeafCount(), 0);
        assertEq(vault.sunsetFinalizedAt(), 0);
        vm.warp(block.timestamp + REVOKE_WINDOW);
        _expectRefundRevert(3, FerrataVault.NotRefundable.selector);

        _sunset(2);
        _expectRefundRevert(3, FerrataVault.RefundNotYetOpen.selector);
        _openRefunds();
        _refund(3);
        assertEq(dai.balanceOf(alice), 10_000e18);
    }

    function test_refund_revokingOnlyLaterCheckpointsKeepsRefunds() public {
        _fourDepositsThenRetire();
        _sunset(2); // checkpoint 0 sets the boundary
        _acceptRoot(keccak256("a later checkpoint")); // checkpoint 1: a sunset checkpoint with the same count
        vm.prank(freezer);
        vault.revokeFrom(1);
        assertTrue(vault.sunsetLeafCountSet());
        _openRefunds();
        _refund(2);
    }

    /// @notice v3 D5: the first sunset checkpoint fixes the boundary and the time refunds open. A later sunset
    ///         checkpoint (it must carry the same count) moves neither; revoking it keeps both, revoking the first
    ///         clears them.
    function test_refund_theFirstSunsetCheckpointFixesTheBoundaryAndTheOpening() public {
        _fourDepositsThenRetire();
        uint256 first = _sunset(2);
        uint256 firstAt = block.timestamp;
        vm.warp(block.timestamp + 1 days);
        uint256 second = _sunset(2);
        assertEq(vault.sunsetSetAt(), first);
        assertEq(vault.sunsetFinalizedAt(), firstAt);

        vm.prank(freezer);
        vault.revokeFrom(second);
        assertTrue(vault.sunsetLeafCountSet());
        assertEq(vault.sunsetLeafCount(), 2);
        vm.warp(firstAt + REVOKE_WINDOW); // the first checkpoint's window, not the second's
        _expectRefundRevert(2, FerrataVault.RefundNotYetOpen.selector); // its last revocable second (D11)

        vm.prank(freezer);
        vault.revokeFrom(first); // still inside its window (the bound is inclusive), so refunds stop again
        assertFalse(vault.sunsetLeafCountSet());
        vm.warp(firstAt + REVOKE_WINDOW + 1);
        _expectRefundRevert(3, FerrataVault.NotRefundable.selector);
    }

    function test_refund_escrowRefundedIsNoLongerReleasable() public {
        _dep(alice, ETH, 1, keccak256("c0")); // leaf 0, 1 ETH
        vm.prank(freezer);
        vault.retire();
        _sunset(0);
        _openRefunds();
        _refund(0);
        assertEq(vault.escrowed(ETH), 0);

        _acceptRoot(keccak256("root"));
        FerrataVault.ReleaseRequest memory r = _request(ETH, 1, recipient, keccak256("root"), keccak256("n"));
        _proveRelease(_releaseJournal(r));
        vm.expectRevert(FerrataVault.InsufficientEscrow.selector);
        vault.release(r, SEAL);
    }

    /// @notice Escrow is per token and shared by releases and refunds. If releases took it (possible only under a burn
    ///         root that should never have been accepted), a refund is refused with its own error, not paid.
    function test_refund_refusedWhenReleasesTookTheEscrow() public {
        _dep(alice, ETH, 1, keccak256("c0")); // leaf 0, imported on LEZ
        _dep(bob, ETH, 1, keccak256("c1")); // leaf 1, never imported
        _acceptRoot(keccak256("root"));
        _release(_request(ETH, 1, recipient, keccak256("root"), keccak256("n1")));
        _release(_request(ETH, 1, recipient, keccak256("root"), keccak256("n2"))); // more than LEZ imported
        vm.prank(freezer);
        vault.retire();
        _sunset(1);
        _openRefunds();
        _expectRefundRevert(1, FerrataVault.InsufficientEscrow.selector);
        assertEq(bob.balance, 0);
    }

    /// @notice SPEC-v3 D9: indices i and i + 2^32 would share a path, since the path is folded over 32 bits. An
    ///         index of 2^32 or more is refused even in a full tree, and the leaf hash binds all 64 bits of the index.
    function test_refund_indexAtOrAbove2To32IsRefused() public {
        _fourDepositsThenRetire();
        _sunset(0);
        _openRefunds();
        Leaf memory l = leafData[2];
        bytes32[32] memory p = _path(2);
        uint64 alias_ = uint64(2 ** 32) + 2;

        vm.expectRevert(FerrataVault.UnknownLeaf.selector);
        vault.refund(alias_, l.depositor, l.token, l.denomIndex, l.commitment, p);
        vm.store(address(vault), bytes32(NEXT_LEAF_INDEX_SLOT_), bytes32(uint256(2 ** 32))); // a full tree
        vm.expectRevert(FerrataVault.UnknownLeaf.selector);
        vault.refund(alias_, l.depositor, l.token, l.denomIndex, l.commitment, p);
        assertTrue(
            _leafHash(alias_, l.depositor, l.token, 0.1 ether, l.commitment)
                != _leafHash(2, l.depositor, l.token, 0.1 ether, l.commitment)
        );
        _refund(2); // the real index still works
        assertFalse(vault.refunded(alias_));
    }

    // ---- when refunds open (v3 D5) --------------------------------------------------------------------------------

    /// @notice Refunds need a retired vault, so the deposit root no longer moves and a refund's path cannot go stale.
    function test_refund_refusedBeforeRetirement() public {
        _dep(alice, ETH, 1, keccak256("c0"));
        _dep(bob, ETH, 0, keccak256("c1"));
        _sunset(1); // leaf 1 was never imported
        _openRefunds();
        _expectRefundRevert(1, FerrataVault.NotRetired.selector);

        vm.prank(freezer);
        vault.retire();
        _refund(1);
        assertEq(bob.balance, 0.1 ether);
    }

    function test_refund_opensRevokeWindowAfterTheSunsetCheckpoint() public {
        _fourDepositsThenRetire();
        uint256 index = _sunset(2);
        uint256 finalizedAt = block.timestamp;
        (,,,,,,,,, uint64 recordedAt,) = vault.checkpoints(index);
        assertEq(recordedAt, finalizedAt);
        assertEq(vault.sunsetFinalizedAt(), finalizedAt);

        _expectRefundRevert(2, FerrataVault.RefundNotYetOpen.selector);
        vm.warp(finalizedAt + REVOKE_WINDOW - 1);
        _expectRefundRevert(2, FerrataVault.RefundNotYetOpen.selector);
        vm.warp(finalizedAt + REVOKE_WINDOW); // the last second the sunset checkpoint can be revoked (D11)
        _expectRefundRevert(2, FerrataVault.RefundNotYetOpen.selector);
        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        _refund(2);
        assertEq(bob.balance, 0.1 ether);
    }

    /// @notice v4 (D11): no second in which both a refund and a revocation of the sunset checkpoint are possible.
    function test_refund_noSecondAllowsBothRefundAndRevocation() public {
        _fourDepositsThenRetire();
        uint256 index = _sunset(2);
        uint256 finalizedAt = block.timestamp;
        vm.warp(finalizedAt + REVOKE_WINDOW);
        _expectRefundRevert(2, FerrataVault.RefundNotYetOpen.selector);
        uint256 snapshot = vm.snapshotState();
        vm.prank(freezer);
        vault.revokeFrom(index); // still possible in that second
        vm.revertToState(snapshot);
        vm.warp(finalizedAt + REVOKE_WINDOW + 1);
        vm.prank(freezer);
        vm.expectRevert(FerrataVault.RevokeWindowPassed.selector);
        vault.revokeFrom(index);
        _refund(2);
        assertTrue(vault.refunded(2));
    }

    /// @notice The global release freeze covers refunds, and lapses like every freeze. A per-token release freeze does
    ///         not (D5 names only the global one).
    function test_refund_refusedUnderTheReleaseFreeze() public {
        _fourDepositsThenRetire();
        _sunset(2);
        _openRefunds();

        vm.prank(freezer);
        vault.setReleasesFrozen(true);
        _expectRefundRevert(2, FerrataVault.ReleasesAreFrozen.selector);
        vm.warp(block.timestamp + MAX_FREEZE - 1);
        _expectRefundRevert(2, FerrataVault.ReleasesAreFrozen.selector);
        vm.warp(block.timestamp + 1);
        _refund(2);
        assertEq(bob.balance, 0.1 ether);

        vm.prank(freezer);
        vault.setTokenReleasesFrozen(address(dai), true);
        _refund(3);
        assertEq(dai.balanceOf(alice), 10_000e18);
    }

    /// @notice v3 D5: a forged sunset checkpoint, here one claiming LEZ imported nothing (so deposits LEZ did mint
    ///         would be paid out twice), is public for `revokeWindow` before any refund. Revoked inside the window, it
    ///         pays nothing, then or later; the honest sunset checkpoint afterwards refunds only what LEZ never imported.
    function test_refund_forgedSunsetRevokedInsideTheWindowPaysNothing() public {
        _fourDepositsThenRetire();
        uint256 forged = _sunset(0);
        for (uint64 i = 0; i < 4; i++) {
            _expectRefundRevert(i, FerrataVault.RefundNotYetOpen.selector);
        }
        vm.warp(block.timestamp + REVOKE_WINDOW - 1);
        for (uint64 i = 0; i < 4; i++) {
            _expectRefundRevert(i, FerrataVault.RefundNotYetOpen.selector);
        }
        vm.prank(freezer);
        vault.revokeFrom(forged);
        assertFalse(vault.sunsetLeafCountSet());

        vm.warp(block.timestamp + 365 days);
        for (uint64 i = 0; i < 4; i++) {
            _expectRefundRevert(i, FerrataVault.NotRefundable.selector);
        }

        _sunset(3); // the honest one: LEZ imported leaves 0 to 2
        _openRefunds();
        for (uint64 i = 0; i < 3; i++) {
            _expectRefundRevert(i, FerrataVault.NotRefundable.selector);
        }
        _refund(3);
        for (uint64 i = 0; i < 3; i++) {
            assertFalse(vault.refunded(i));
        }
        assertEq(vault.escrowed(ETH), 1.1 ether);
        assertEq(vault.escrowed(address(usdc)), 1000e6);
        assertEq(vault.escrowed(address(dai)), 0);
        assertEq(alice.balance + bob.balance + usdc.balanceOf(bob), 0);
    }

    /// @notice v4 (D13): refunds draw on the release buckets, the token's and the global one.
    function test_refund_drawsOnTheReleaseBuckets() public {
        _fourDepositsThenRetire();
        _sunset(2);
        _openRefunds();
        vm.prank(admin);
        vault.setGlobalCap(FerrataVault.Direction.Release, 3e8 - 1, 0); // 0.1 ether is 3e8 value units
        _expectRefundRevert(2, FerrataVault.GlobalReleaseCapExceeded.selector);
        vm.startPrank(admin);
        vault.setGlobalCap(FerrataVault.Direction.Release, UNLIMITED, 0);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 0.1 ether - 1, 0);
        vm.stopPrank();
        _expectRefundRevert(2, FerrataVault.TokenReleaseCapExceeded.selector);
        vm.startPrank(admin);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, UNLIMITED, 0);
        vault.setTokenCap(ETH, FerrataVault.Direction.Release, 0.1 ether, 0); // from UNLIMITED: starts full
        vault.setGlobalCap(FerrataVault.Direction.Release, 3e8, 0);
        vm.stopPrank();
        _refund(2);
        (,, uint256 tokenLeft) = vault.tokenCap(ETH, FerrataVault.Direction.Release);
        (,, uint256 globalLeft) = vault.globalCap(FerrataVault.Direction.Release);
        assertEq(tokenLeft, 0);
        assertEq(globalLeft, 0);
    }
}
