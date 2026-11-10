// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {MockERC20, MockTokenBase} from "./mocks/MockTokens.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

contract FreezeTest is VaultTestBase {
    bytes32 internal constant ROOT = keccak256("burn root");
    uint256 internal nonce;

    function setUp() public override {
        super.setUp();
        for (uint256 i = 0; i < 4; i++) {
            _deposit(alice, ETH, 0, keccak256(abi.encode("eth", i)));
            _deposit(alice, address(dai), 0, keccak256(abi.encode("dai", i)));
        }
        _acceptRoot(ROOT);
    }

    // ---- attempt helpers: return (success, revert data) instead of reverting ------------------------------------

    function _tryDeposit(address token) internal returns (bool ok, bytes memory ret) {
        uint256 amount = _amount(token, 0);
        uint64 index = vault.depositCount(bob);
        bytes32 commitment = keccak256(abi.encode("bob", index));
        if (token == ETH) {
            vm.deal(bob, amount);
            vm.prank(bob);
            (ok, ret) =
                address(vault).call{value: amount}(abi.encodeCall(FerrataVault.depositETH, (0, commitment, index)));
        } else {
            MockTokenBase(token).mint(bob, amount);
            vm.prank(bob);
            MockTokenBase(token).approve(address(vault), amount);
            vm.prank(bob);
            (ok, ret) = address(vault).call(abi.encodeCall(FerrataVault.deposit, (token, 0, commitment, index)));
        }
    }

    function _tryRelease(address token) internal returns (bool ok, bytes memory ret) {
        FerrataVault.ReleaseRequest memory r = _request(token, 0, recipient, ROOT, keccak256(abi.encode("n", nonce++)));
        _proveRelease(_releaseJournal(r));
        vm.prank(relayer);
        (ok, ret) = address(vault).call(abi.encodeCall(FerrataVault.release, (r, SEAL)));
    }

    function _tryAcceptRoot() internal returns (bool ok, bytes memory ret) {
        bytes32 root = keccak256(abi.encode("root", nonce++));
        _proveRoot(_rootJournal(root, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH));
        (ok, ret) = address(vault).call(
            abi.encodeCall(FerrataVault.acceptBurnRoot, (root, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL))
        );
    }

    /// @dev expectedError == 0 means the call must succeed; otherwise it must fail with exactly that error.
    function _assertOutcome(bool ok, bytes memory ret, bytes4 expectedError, string memory what) internal pure {
        if (expectedError == bytes4(0)) {
            assertTrue(ok, what);
        } else {
            assertFalse(ok, what);
            assertEq(ret, abi.encodeWithSelector(expectedError), what);
        }
    }

    // ---- tests ----------------------------------------------------------------------------------------------------

    /// @notice All 16 combinations of {global deposits, global releases, DAI deposits, DAI releases} frozen.
    ///         ETH (no per-token freeze) is the bystander that shows per-token freezes stay per token.
    function test_freeze_everyCombination() public {
        for (uint256 mask = 0; mask < 16; mask++) {
            bool globalDeposits = (mask & 1) != 0;
            bool globalReleases = (mask & 2) != 0;
            bool daiDeposits = (mask & 4) != 0;
            bool daiReleases = (mask & 8) != 0;

            uint256 snapshot = vm.snapshotState();
            vm.startPrank(freezer);
            vault.setDepositsFrozen(globalDeposits);
            vault.setReleasesFrozen(globalReleases);
            vault.setTokenDepositsFrozen(address(dai), daiDeposits);
            vault.setTokenReleasesFrozen(address(dai), daiReleases);
            vm.stopPrank();

            bytes4 ethDepositError = globalDeposits ? FerrataVault.DepositsAreFrozen.selector : bytes4(0);
            bytes4 daiDepositError = globalDeposits
                ? FerrataVault.DepositsAreFrozen.selector
                : (daiDeposits ? FerrataVault.TokenDepositsAreFrozen.selector : bytes4(0));
            bytes4 ethReleaseError = globalReleases ? FerrataVault.ReleasesAreFrozen.selector : bytes4(0);
            bytes4 daiReleaseError = globalReleases
                ? FerrataVault.ReleasesAreFrozen.selector
                : (daiReleases ? FerrataVault.TokenReleasesAreFrozen.selector : bytes4(0));

            (bool ok, bytes memory ret) = _tryDeposit(ETH);
            _assertOutcome(ok, ret, ethDepositError, "ETH deposit");
            (ok, ret) = _tryDeposit(address(dai));
            _assertOutcome(ok, ret, daiDepositError, "DAI deposit");
            (ok, ret) = _tryRelease(ETH);
            _assertOutcome(ok, ret, ethReleaseError, "ETH release");
            (ok, ret) = _tryRelease(address(dai));
            _assertOutcome(ok, ret, daiReleaseError, "DAI release");
            // Root acceptance follows the global release freeze only.
            (ok, ret) = _tryAcceptRoot();
            _assertOutcome(ok, ret, ethReleaseError, "acceptBurnRoot");

            vm.revertToState(snapshot);
        }
    }

    function test_freeze_settersEmitAndSetFlags() public {
        vm.startPrank(freezer);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.DepositsFrozenSet(true);
        vault.setDepositsFrozen(true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.ReleasesFrozenSet(true);
        vault.setReleasesFrozen(true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenDepositsFrozenSet(address(usdc), true);
        vault.setTokenDepositsFrozen(address(usdc), true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenReleasesFrozenSet(address(usdc), true);
        vault.setTokenReleasesFrozen(address(usdc), true);
        vm.stopPrank();

        assertTrue(vault.depositsFrozen());
        assertTrue(vault.releasesFrozen());
        assertTrue(vault.tokenDepositsFrozen(address(usdc)));
        assertTrue(vault.tokenReleasesFrozen(address(usdc)));
        assertFalse(vault.tokenDepositsFrozen(address(dai)));
    }

    function test_freeze_perTokenRequiresRegisteredToken() public {
        MockERC20 other = new MockERC20("Other", "OTH", 18);
        vm.startPrank(freezer);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setTokenDepositsFrozen(address(other), true);
        vm.expectRevert(FerrataVault.TokenNotRegistered.selector);
        vault.setTokenReleasesFrozen(address(other), true);
        vm.stopPrank();
    }

    function test_freeze_delaysButNeverInvalidatesAClaim() public {
        FerrataVault.ReleaseRequest memory r = _request(ETH, 0, recipient, ROOT, keccak256("n"));
        _proveRelease(_releaseJournal(r));
        vm.prank(freezer);
        vault.setReleasesFrozen(true);
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.release(r, SEAL);

        vm.warp(block.timestamp + 365 days);
        vm.prank(freezer);
        vault.setReleasesFrozen(false);
        vault.release(r, SEAL); // the same proof, a year later
        assertEq(recipient.balance, 0.1 ether);
    }

    function test_freeze_releaseDelayRunsFromAcceptanceAfterUnfreeze() public {
        vm.prank(admin);
        vault.setReleaseDelay(ETH, 1 days);
        vm.prank(freezer);
        vault.setReleasesFrozen(true);

        bytes32 root = keccak256("root proven during the freeze");
        _proveRoot(_rootJournal(root, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH));
        vm.expectRevert(FerrataVault.ReleasesAreFrozen.selector);
        vault.acceptBurnRoot(root, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);

        vm.warp(block.timestamp + 2 days); // longer than the delay
        vm.prank(freezer);
        vault.setReleasesFrozen(false);
        vault.acceptBurnRoot(root, LEZ_BLOCK, BURN_COUNT, MIN_DEPTH, SEAL);
        assertEq(vault.burnRootAcceptedAt(root), block.timestamp);

        FerrataVault.ReleaseRequest memory r = _request(ETH, 0, recipient, root, keccak256("n"));
        _proveRelease(_releaseJournal(r));
        vm.expectRevert(FerrataVault.ReleaseDelayNotElapsed.selector);
        vault.release(r, SEAL); // the full delay applies after the unfreeze
        vm.warp(block.timestamp + 1 days);
        vault.release(r, SEAL);
    }
}
