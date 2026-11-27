// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {IRiscZeroVerifier} from "../src/interfaces/IRiscZeroVerifier.sol";
import {MockReentrantToken} from "./mocks/MockTokens.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice The internal review of 27 November 2026 (docs/security/internal-review-2026-11-27.md): one test per finding
///         fixed in the vault, each failing on the tree the review read (50b396a8) and passing after the fix.
contract InternalReviewTest is VaultTestBase {
    bytes32 internal constant C = bytes32(uint256(0xc0ffee));

    /// @dev A registered token whose transferFrom first calls a hook, as an ERC-777 token calls the sender's
    ///      `tokensToSend` hook before it moves the balance; funded and approved for one deposit of `denomIndex` 0.
    function _hookToken(FerrataVault v, address depositor) internal returns (MockReentrantToken token) {
        token = new MockReentrantToken();
        vm.prank(admin);
        v.registerToken(address(token), _params(_denoms(100e18, 1000e18, 10_000e18), 1e12, 1));
        uint256 total = _depositTotal(v, address(token), 0);
        token.mint(depositor, total);
        vm.prank(depositor);
        token.approve(address(v), total);
    }

    // ------------------------------------------------------------------------------------------------------------
    // S-1: a token hook runs inside `deposit` between its checks and its leaf, and can retire the vault or finalise a
    // sunset checkpoint there; the deposit then lands after the state that should have refused it.
    // ------------------------------------------------------------------------------------------------------------

    /// @notice Keyless retirement is open; the token's hook calls `retire()` during the deposit's pull. The deposit must
    ///         be refused like any deposit after `retire()`, not appended to a retired vault's tree.
    function test_S1_aTokenHookCannotLandADepositAfterRetirement() public {
        MockReentrantToken token = _hookToken(vault, alice);
        vm.warp(vault.retireOpensAt());
        token.setHook(address(vault), abi.encodeCall(FerrataVault.retire, ()));

        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vault.deposit(address(token), 0, C, 0);

        assertEq(vault.nextLeafIndex(), 0, "no leaf");
        assertFalse(vault.retired(), "the reverted deposit took the hook's retirement with it");
        assertEq(token.balanceOf(address(vault)), 0, "nothing taken");
    }

    /// @notice The same with the permissionless finalisation of a sunset checkpoint (rule (B)) in the hook: after a
    ///         final sunset checkpoint LEZ imports nothing more, and the vault refuses deposits (review L-3).
    function test_S1_aTokenHookCannotLandADepositAfterASunsetCheckpoint() public {
        FerrataVault vB = new FerrataVault(_deploymentB(IRiscZeroVerifier(address(estop)), LEZ_CONFIG_ID));
        MockReentrantToken token = _hookToken(vB, alice);
        FerrataVault.Checkpoint memory c = _nextCheckpoint(vB, keccak256("burn root"), 0, true);
        _submit(vB, LEZ_CONFIG_ID, c);
        (uint256 submittedAt, uint256 wait) = _waitOf(vB, c.newState);
        vm.warp(submittedAt + wait);
        token.setHook(address(vB), abi.encodeCall(FerrataVault.finalizeAttested, (c)));

        vm.prank(alice);
        vm.expectRevert(FerrataVault.VaultRetired.selector);
        vB.deposit(address(token), 0, C, 0);

        assertEq(vB.nextLeafIndex(), 0, "no leaf");
        assertFalse(vB.finalSunset(), "the reverted deposit took the hook's finalisation with it");
    }

    /// @notice Control: the same hook with nothing to change (retirement not open) leaves an ordinary deposit, so the
    ///         two tests above fail on the hook's effect, not on the hook itself.
    function test_S1_control_aHookThatChangesNothingLeavesAnOrdinaryDeposit() public {
        MockReentrantToken token = _hookToken(vault, alice);
        token.setHook(address(vault), abi.encodeCall(FerrataVault.retire, ()));

        vm.prank(alice);
        vault.deposit(address(token), 0, C, 0);

        assertTrue(token.hookCalled(), "the hook ran");
        assertFalse(token.hookSucceeded(), "retire() was refused inside the hook (RetireNotOpen)");
        assertEq(vault.nextLeafIndex(), 1, "one leaf");
        assertEq(vault.escrowed(address(token)), _depositTotal(vault, address(token), 0), "escrowed");
    }
}
