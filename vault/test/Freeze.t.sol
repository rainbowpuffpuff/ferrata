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
        uint256 amount = _depositTotal(vault, token, 0); // the denomination and its fees (v5)
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

    function _trySubmitCandidate() internal returns (bool ok, bytes memory ret) {
        FerrataVault.Checkpoint memory c = _nextCheckpoint(keccak256(abi.encode("root", nonce++)));
        _proveCheckpoint(_checkpointJournal(c));
        (ok, ret) = address(vault).call(abi.encodeCall(FerrataVault.submitCandidate, (c, SEAL)));
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

    /// @notice All 32 combinations of {global deposits, global releases, DAI deposits, DAI releases, checkpoints}
    ///         frozen. ETH (no per-token freeze) is the bystander that shows per-token freezes stay per token, and each
    ///         freeze stops exactly its own path. (The checkpoint freeze also stops finalisation, and the release freeze
    ///         refunds: Checkpoint.t.sol and Refund.t.sol.)
    function test_freeze_everyCombination() public {
        for (uint256 mask = 0; mask < 32; mask++) {
            bool globalDeposits = (mask & 1) != 0;
            bool globalReleases = (mask & 2) != 0;
            bool daiDeposits = (mask & 4) != 0;
            bool daiReleases = (mask & 8) != 0;
            bool candidates = (mask & 16) != 0;

            uint256 snapshot = vm.snapshotState();
            vm.startPrank(freezer);
            vault.setDepositsFrozen(globalDeposits);
            vault.setReleasesFrozen(globalReleases);
            vault.setTokenDepositsFrozen(address(dai), daiDeposits);
            vault.setTokenReleasesFrozen(address(dai), daiReleases);
            vault.setCheckpointsFrozen(candidates);
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
            (ok, ret) = _trySubmitCandidate();
            _assertOutcome(ok, ret, candidates ? FerrataVault.CheckpointsAreFrozen.selector : bytes4(0), "candidate");

            vm.revertToState(snapshot);
        }
    }

    function test_freeze_settersEmitAndSetFlags() public {
        uint256 until = block.timestamp + MAX_FREEZE;
        vm.startPrank(freezer);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.DepositsFrozenSet(true, until);
        vault.setDepositsFrozen(true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.ReleasesFrozenSet(true, until);
        vault.setReleasesFrozen(true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenDepositsFrozenSet(address(usdc), true, until);
        vault.setTokenDepositsFrozen(address(usdc), true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.TokenReleasesFrozenSet(address(usdc), true, until);
        vault.setTokenReleasesFrozen(address(usdc), true);
        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.CheckpointsFrozenSet(true, until);
        vault.setCheckpointsFrozen(true);
        vm.stopPrank();

        assertTrue(_isFrozen(vault.depositsFrozenUntil()));
        assertTrue(_isFrozen(vault.releasesFrozenUntil()));
        assertTrue(_isFrozen(vault.tokenDepositsFrozenUntil(address(usdc))));
        assertTrue(_isFrozen(vault.tokenReleasesFrozenUntil(address(usdc))));
        assertTrue(_isFrozen(vault.checkpointsFrozenUntil()));
        assertFalse(_isFrozen(vault.tokenDepositsFrozenUntil(address(dai))));
        assertEq(vault.depositsFrozenUntil(), until);

        vm.expectEmit(true, true, true, true, address(vault));
        emit FerrataVault.DepositsFrozenSet(false, 0);
        vm.prank(freezer);
        vault.setDepositsFrozen(false);
        assertFalse(_isFrozen(vault.depositsFrozenUntil()));
    }

    // ---- expiry (spec §7): a freeze lasts maxFreeze unless renewed ------------------------------------------------

    /// @dev Freezes everything at once and returns the time the freezes lapse.
    function _freezeEverything() internal returns (uint256 until) {
        until = block.timestamp + MAX_FREEZE;
        vm.startPrank(freezer);
        vault.setDepositsFrozen(true);
        vault.setReleasesFrozen(true);
        vault.setCheckpointsFrozen(true);
        vault.setTokenDepositsFrozen(address(dai), true);
        vault.setTokenReleasesFrozen(address(dai), true);
        vm.stopPrank();
    }

    function test_freeze_everyKindExpiresAfterMaxFreeze() public {
        uint256 until = _freezeEverything();

        vm.warp(until - 1); // the last frozen second
        (bool ok,) = _tryDeposit(address(dai));
        assertFalse(ok);
        (ok,) = _tryRelease(address(dai));
        assertFalse(ok);
        (ok,) = _trySubmitCandidate();
        assertFalse(ok);

        vm.warp(until); // lapsed, with nobody doing anything
        assertFalse(_isFrozen(vault.depositsFrozenUntil()));
        assertFalse(_isFrozen(vault.releasesFrozenUntil()));
        assertFalse(_isFrozen(vault.checkpointsFrozenUntil()));
        assertFalse(_isFrozen(vault.tokenDepositsFrozenUntil(address(dai))));
        assertFalse(_isFrozen(vault.tokenReleasesFrozenUntil(address(dai))));
        (ok,) = _tryDeposit(address(dai));
        assertTrue(ok);
        (ok,) = _tryRelease(address(dai));
        assertTrue(ok);
        (ok,) = _trySubmitCandidate();
        assertTrue(ok);
    }

    function test_freeze_renewingRestartsTheClock() public {
        uint256 first = _freezeEverything();
        vm.warp(first - 1 days);
        uint256 second = _freezeEverything(); // renewed before it lapsed
        assertEq(second, first - 1 days + MAX_FREEZE);
        vm.warp(first);
        (bool ok, bytes memory ret) = _tryRelease(ETH);
        assertFalse(ok);
        assertEq(ret, abi.encodeWithSelector(FerrataVault.ReleasesAreFrozen.selector));
        vm.warp(second);
        (ok,) = _tryRelease(ETH);
        assertTrue(ok);
    }

    function test_freeze_aLostFreezeKeyCannotLockTheVaultForever() public {
        _freezeEverything();
        // The freeze authority's key is lost: nobody can unfreeze, but every freeze still lapses.
        vm.warp(block.timestamp + MAX_FREEZE);
        (bool ok,) = _tryDeposit(ETH);
        assertTrue(ok);
        (ok,) = _tryRelease(ETH);
        assertTrue(ok);
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
}
