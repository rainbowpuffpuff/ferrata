// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import {FerrataVault} from "../src/FerrataVault.sol";
import {
    MockERC20, MockFalseReturnToken, MockFeeOnTransferToken, MockTokenBase, MockUSDT
} from "./mocks/MockTokens.sol";
import {ReferenceMerkle} from "./utils/ReferenceMerkle.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

contract DepositTest is VaultTestBase {
    bytes32 internal constant C1 = keccak256("commitment 1");
    bytes32 internal constant C2 = keccak256("commitment 2");

    function _rootOfSingleLeaf(bytes32 leaf) internal pure returns (bytes32) {
        bytes32[] memory leaves = new bytes32[](1);
        leaves[0] = leaf;
        return ReferenceMerkle.root(leaves);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Happy paths: ETH, 6- and 18-decimal tokens, USDT-style token
    // ------------------------------------------------------------------------------------------------------------

    function test_depositETH_escrowsAndAppendsLeaf() public {
        uint256 amount = 1 ether;
        vm.deal(alice, amount);

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Deposit(alice, 0, 0, ETH, amount, C1);
        vm.prank(alice);
        vault.depositETH{value: amount}(1, C1, 0);

        assertEq(address(vault).balance, amount);
        assertEq(alice.balance, 0);
        assertEq(vault.escrowed(ETH), amount);
        assertEq(vault.depositCount(alice), 1);
        assertEq(vault.nextLeafIndex(), 1);
        assertEq(vault.depositRoot(), _rootOfSingleLeaf(ReferenceMerkle.leafHash(0, alice, ETH, amount, C1)));
    }

    function test_deposit_sixDecimalToken() public {
        _assertTokenDeposit(address(usdc), 0);
    }

    function test_deposit_eighteenDecimalToken() public {
        _assertTokenDeposit(address(dai), 2);
    }

    function test_deposit_usdtStyleTokenWithoutReturnValue() public {
        _assertTokenDeposit(address(usdt), 1);
    }

    function _assertTokenDeposit(address token, uint8 denomIndex) internal {
        uint256 amount = _amount(token, denomIndex);
        MockTokenBase(token).mint(alice, amount);
        vm.prank(alice);
        MockTokenBase(token).approve(address(vault), amount);

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.Deposit(alice, 0, 0, token, amount, C1);
        vm.prank(alice);
        vault.deposit(token, denomIndex, C1, 0);

        assertEq(MockTokenBase(token).balanceOf(address(vault)), amount);
        assertEq(MockTokenBase(token).balanceOf(alice), 0);
        assertEq(vault.escrowed(token), amount);
        assertEq(vault.depositCount(alice), 1);
        assertEq(vault.depositRoot(), _rootOfSingleLeaf(ReferenceMerkle.leafHash(0, alice, token, amount, C1)));
    }

    function test_leafHash_matchesSpec() public view {
        bytes32 expected = sha256(
            bytes.concat(
                bytes1(0x00),
                sha256(bytes("ferrata/vault/leaf/v2")),
                bytes8(uint64(7)),
                bytes20(alice),
                bytes20(address(usdc)),
                bytes32(uint256(1000e6)),
                C1
            )
        );
        assertEq(vault.leafHash(7, alice, address(usdc), 1000e6, C1), expected);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Rejected tokens
    // ------------------------------------------------------------------------------------------------------------

    function test_deposit_feeOnTransferTokenReverts() public {
        MockFeeOnTransferToken fot = new MockFeeOnTransferToken();
        vm.prank(admin);
        vault.registerToken(address(fot), _params(_denoms(1e18, 2e18, 3e18), 1e12, 1));
        fot.mint(alice, 1e18);
        vm.prank(alice);
        fot.approve(address(vault), 1e18);

        vm.prank(alice);
        vm.expectRevert(FerrataVault.TransferAmountMismatch.selector);
        vault.deposit(address(fot), 0, C1, 0);
    }

    function test_deposit_falseReturningTokenReverts() public {
        MockFalseReturnToken token = new MockFalseReturnToken();
        vm.prank(admin);
        vault.registerToken(address(token), _params(_denoms(1e18, 2e18, 3e18), 1e12, 1));
        token.mint(alice, 1e18);
        vm.prank(alice);
        token.approve(address(vault), 1e18);

        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(SafeERC20.SafeERC20FailedOperation.selector, address(token)));
        vault.deposit(address(token), 0, C1, 0);
    }

    function test_deposit_unregisteredTokenReverts() public {
        MockERC20 other = new MockERC20("Other", "OTH", 18);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.deposit(address(other), 0, C1, 0);
    }

    function test_deposit_disabledTokenRevertsAndReenableWorks() public {
        vm.prank(admin);
        vault.setDepositsEnabled(address(dai), false);
        dai.mint(alice, 100e18);
        vm.prank(alice);
        dai.approve(address(vault), 100e18);

        vm.prank(alice);
        vm.expectRevert(FerrataVault.DepositsDisabled.selector);
        vault.deposit(address(dai), 0, C1, 0);

        vm.prank(admin);
        vault.setDepositsEnabled(address(dai), true);
        vm.prank(alice);
        vault.deposit(address(dai), 0, C1, 0);
        assertEq(vault.escrowed(address(dai)), 100e18);
    }

    function test_depositETH_disabledReverts() public {
        vm.prank(admin);
        vault.setDepositsEnabled(ETH, false);
        vm.deal(alice, 1 ether);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.DepositsDisabled.selector);
        vault.depositETH{value: 1 ether}(1, C1, 0);
    }

    function test_deposit_ethSentinelThroughErc20PathReverts() public {
        vm.prank(alice);
        vm.expectRevert(FerrataVault.UseDepositETH.selector);
        vault.deposit(ETH, 0, C1, 0);
    }

    function test_deposit_erc20PathIsNotPayable() public {
        _fund(address(usdc), alice, 100e6);
        vm.deal(alice, 1 ether);
        vm.prank(alice);
        (bool ok,) = address(vault).call{value: 1}(abi.encodeCall(FerrataVault.deposit, (address(usdc), 0, C1, 0)));
        assertFalse(ok);
        assertEq(vault.nextLeafIndex(), 0);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Argument checks
    // ------------------------------------------------------------------------------------------------------------

    function test_depositETH_wrongValueReverts() public {
        vm.deal(alice, 10 ether);
        uint256[3] memory wrong = [uint256(0), 1 ether - 1, 1 ether + 1];
        for (uint256 i = 0; i < wrong.length; i++) {
            vm.prank(alice);
            vm.expectRevert(FerrataVault.WrongEthAmount.selector);
            vault.depositETH{value: wrong[i]}(1, C1, 0);
        }
    }

    function test_deposit_wrongDenominationIndexReverts() public {
        _fund(address(usdc), alice, 10_000e6);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.InvalidDenominationIndex.selector);
        vault.deposit(address(usdc), 3, C1, 0);

        vm.prank(alice);
        vm.expectRevert(FerrataVault.InvalidDenominationIndex.selector);
        vault.deposit(address(usdc), type(uint8).max, C1, 0);

        vm.deal(alice, 10 ether);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.InvalidDenominationIndex.selector);
        vault.depositETH{value: 1 ether}(3, C1, 0);
    }

    function test_deposit_zeroCommitmentReverts() public {
        _fund(address(usdc), alice, 100e6);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.ZeroCommitment.selector);
        vault.deposit(address(usdc), 0, bytes32(0), 0);

        vm.deal(alice, 1 ether);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.ZeroCommitment.selector);
        vault.depositETH{value: 1 ether}(1, bytes32(0), 0);
    }

    function test_deposit_wrongIndexReverts() public {
        vm.deal(alice, 10 ether);
        // Index ahead of the counter.
        vm.prank(alice);
        vm.expectRevert(FerrataVault.WrongDepositIndex.selector);
        vault.depositETH{value: 1 ether}(1, C1, 1);

        vm.prank(alice);
        vault.depositETH{value: 1 ether}(1, C1, 0);

        // Replaying index 0 (e.g. a resubmitted transaction).
        vm.prank(alice);
        vm.expectRevert(FerrataVault.WrongDepositIndex.selector);
        vault.depositETH{value: 1 ether}(1, C1, 0);

        // Skipping ahead.
        vm.prank(alice);
        vm.expectRevert(FerrataVault.WrongDepositIndex.selector);
        vault.depositETH{value: 1 ether}(1, C1, 2);

        vm.prank(alice);
        vault.depositETH{value: 1 ether}(1, C1, 1);
        assertEq(vault.depositCount(alice), 2);
    }

    function test_deposit_indexIsPerDepositor() public {
        vm.deal(alice, 10 ether);
        vm.deal(bob, 10 ether);
        vm.prank(alice);
        vault.depositETH{value: 1 ether}(1, C1, 0);
        vm.prank(bob);
        vault.depositETH{value: 1 ether}(1, C1, 0);
        vm.prank(alice);
        vault.depositETH{value: 1 ether}(1, C1, 1);

        assertEq(vault.depositCount(alice), 2);
        assertEq(vault.depositCount(bob), 1);
        assertEq(vault.nextLeafIndex(), 3);
    }

    function test_deposit_repeatedCommitmentIsAllowed() public {
        // Bob copies Alice's commitment. That must not block Alice, and it creates a distinct leaf (which also
        // names Bob as its depositor).
        uint64 first = _deposit(alice, address(usdc), 0, C1);
        uint64 second = _deposit(bob, address(usdc), 0, C1);
        uint64 third = _deposit(alice, address(usdc), 0, C1);
        assertEq(first, 0);
        assertEq(second, 1);
        assertEq(third, 2);

        bytes32[] memory leaves = new bytes32[](3);
        leaves[0] = ReferenceMerkle.leafHash(0, alice, address(usdc), 100e6, C1);
        leaves[1] = ReferenceMerkle.leafHash(1, bob, address(usdc), 100e6, C1);
        leaves[2] = ReferenceMerkle.leafHash(2, alice, address(usdc), 100e6, C1);
        assertTrue(leaves[0] != leaves[1]);
        assertEq(vault.depositRoot(), ReferenceMerkle.root(leaves));
    }

    function test_deposit_revertsWhenTreeIsFull() public {
        vm.store(address(vault), bytes32(vault.NEXT_LEAF_INDEX_SLOT()), bytes32(uint256(2 ** 32)));
        vm.deal(alice, 1 ether);
        vm.prank(alice);
        vm.expectRevert(FerrataVault.TreeFull.selector);
        vault.depositETH{value: 1 ether}(1, C1, 0);
    }

    // ------------------------------------------------------------------------------------------------------------
    // Plain ETH and unknown calls
    // ------------------------------------------------------------------------------------------------------------

    function test_receive_rejectsPlainEth() public {
        vm.deal(alice, 1 ether);
        vm.prank(alice);
        (bool ok, bytes memory ret) = address(vault).call{value: 1 ether}("");
        assertFalse(ok);
        assertEq(ret, abi.encodeWithSelector(FerrataVault.DirectEthNotAccepted.selector));
        assertEq(address(vault).balance, 0);

        vm.prank(alice);
        bool sent = payable(address(vault)).send(1 ether);
        assertFalse(sent);
        assertEq(alice.balance, 1 ether);
    }

    function test_fallback_rejectsUnknownCalls() public {
        vm.deal(alice, 1 ether);
        vm.prank(alice);
        (bool ok, bytes memory ret) = address(vault).call{value: 1 ether}(hex"deadbeef");
        assertFalse(ok);
        assertEq(ret, abi.encodeWithSelector(FerrataVault.FunctionNotFound.selector));

        (ok, ret) = address(vault).call(hex"deadbeef");
        assertFalse(ok);
        assertEq(ret, abi.encodeWithSelector(FerrataVault.FunctionNotFound.selector));
    }

    function _fund(address token, address who, uint256 amount) internal {
        MockTokenBase(token).mint(who, amount);
        vm.prank(who);
        MockTokenBase(token).approve(address(vault), amount);
    }

    /// Fixture check: the USDT-style mock really returns no data, so the USDT tests exercise SafeERC20's
    /// no-return-value path.
    function test_usdtMock_hasNoReturnValue() public {
        _fund(address(usdt), alice, 1);
        vm.prank(alice);
        (bool ok, bytes memory ret) = address(usdt).call(abi.encodeCall(MockUSDT.transfer, (bob, 1)));
        assertTrue(ok);
        assertEq(ret.length, 0);
    }
}
