// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.26;

import {FerrataVault} from "../src/FerrataVault.sol";
import {MockTokenBase} from "./mocks/MockTokens.sol";
import {VaultTestBase} from "./utils/VaultTestBase.sol";

/// @notice Token-bucket caps. Value unit = 1e-6 USD (see VaultTestBase): USDC and DAI weigh 1 unit per 1e-6 token,
///         ETH 3000 units per 1e-6 ETH.
contract CapsTest is VaultTestBase {
    FerrataVault.Direction internal constant DEPOSIT = FerrataVault.Direction.Deposit;
    FerrataVault.Direction internal constant RELEASE = FerrataVault.Direction.Release;
    bytes32 internal constant C = keccak256("commitment");
    bytes32 internal constant ROOT = keccak256("burn root");
    uint256 internal nullifierNonce;

    /// @dev Caps are tested on tokens without a relay fee, so the bucket arithmetic reads in whole denominations;
    ///      that the buckets count the whole amount paid, fees included, is tested in Fees.t.sol.
    function setUp() public override {
        relayFeeDivisor = 0;
        super.setUp();
    }

    function _setTokenCap(address token, FerrataVault.Direction d, uint256 capacity, uint256 refill) internal {
        vm.prank(admin);
        vault.setTokenCap(token, d, capacity, refill);
    }

    function _setGlobalCap(FerrataVault.Direction d, uint256 capacity, uint256 refill) internal {
        vm.prank(admin);
        vault.setGlobalCap(d, capacity, refill);
    }

    function _available(address token, FerrataVault.Direction d) internal view returns (uint256 available) {
        (,, available) = vault.tokenCap(token, d);
    }

    function _globalAvailable(FerrataVault.Direction d) internal view returns (uint256 available) {
        (,, available) = vault.globalCap(d);
    }

    /// @dev Attempts a deposit from alice. On failure, the error must be `allowedError`.
    function _tryDeposit(address token, uint8 denomIndex, bytes4 allowedError) internal returns (bool ok) {
        uint256 amount = _amount(token, denomIndex);
        uint64 index = vault.depositCount(alice);
        bytes memory ret;
        if (token == ETH) {
            vm.deal(alice, amount);
            vm.prank(alice);
            (ok, ret) =
                address(vault).call{value: amount}(abi.encodeCall(FerrataVault.depositETH, (denomIndex, C, index)));
        } else {
            MockTokenBase(token).mint(alice, amount);
            vm.prank(alice);
            MockTokenBase(token).approve(address(vault), amount);
            vm.prank(alice);
            (ok, ret) = address(vault).call(abi.encodeCall(FerrataVault.deposit, (token, denomIndex, C, index)));
        }
        if (!ok) assertEq(ret, abi.encodeWithSelector(allowedError));
    }

    function _expectDepositRevert(address token, uint8 denomIndex, bytes4 expectedError) internal {
        uint64 leavesBefore = vault.nextLeafIndex();
        assertFalse(_tryDeposit(token, denomIndex, expectedError));
        assertEq(vault.nextLeafIndex(), leavesBefore);
    }

    function _newRelease(address token, uint8 denomIndex) internal returns (FerrataVault.ReleaseRequest memory r) {
        r = _request(token, denomIndex, recipient, ROOT, keccak256(abi.encode("nullifier", nullifierNonce++)));
        _proveRelease(_releaseJournal(r));
    }

    // ---- per-token deposit cap ------------------------------------------------------------------------------------

    function test_tokenDepositCap_exhaustsAndRefillsOverTime() public {
        _setTokenCap(address(dai), DEPOSIT, 200e18, 1e18); // 200 DAI burst, refills 1 DAI per second
        _deposit(alice, address(dai), 0, C);
        _deposit(alice, address(dai), 0, C);
        assertEq(_available(address(dai), DEPOSIT), 0);
        _expectDepositRevert(address(dai), 0, FerrataVault.TokenDepositCapExceeded.selector);

        vm.warp(block.timestamp + 99);
        assertEq(_available(address(dai), DEPOSIT), 99e18);
        _expectDepositRevert(address(dai), 0, FerrataVault.TokenDepositCapExceeded.selector);

        vm.warp(block.timestamp + 1);
        _deposit(alice, address(dai), 0, C);
        assertEq(_available(address(dai), DEPOSIT), 0);

        vm.warp(block.timestamp + 10 days); // refill saturates at capacity
        assertEq(_available(address(dai), DEPOSIT), 200e18);
    }

    function test_tokenDepositCap_rejectionConsumesNothing() public {
        _setTokenCap(address(dai), DEPOSIT, 150e18, 0);
        _expectDepositRevert(address(dai), 1, FerrataVault.TokenDepositCapExceeded.selector); // 1000 > 150
        assertEq(_available(address(dai), DEPOSIT), 150e18);
        _deposit(alice, address(dai), 0, C); // 100 still fits
        assertEq(_available(address(dai), DEPOSIT), 50e18);
    }

    // ---- global deposit cap ---------------------------------------------------------------------------------------

    function test_globalDepositCap_countsValueAcrossTokens() public {
        _setGlobalCap(DEPOSIT, 1500e6, 0); // 1500 USD
        _deposit(alice, address(usdc), 1, C); // 1000 USD
        assertEq(_globalAvailable(DEPOSIT), 500e6);
        _deposit(alice, ETH, 0, C); // 0.1 ETH = 300 USD
        assertEq(_globalAvailable(DEPOSIT), 200e6);

        _expectDepositRevert(address(dai), 1, FerrataVault.GlobalDepositCapExceeded.selector); // 1000 USD
        _expectDepositRevert(ETH, 0, FerrataVault.GlobalDepositCapExceeded.selector); // 300 USD
        _deposit(alice, address(dai), 0, C); // 100 USD fits
        assertEq(_globalAvailable(DEPOSIT), 100e6);
    }

    function test_globalDepositCapRejection_leavesTokenBucketUntouched() public {
        _setTokenCap(address(dai), DEPOSIT, 5000e18, 0);
        _setGlobalCap(DEPOSIT, 500e6, 0);
        // Passes the DAI bucket, fails the global bucket: the DAI bucket must not be charged.
        _expectDepositRevert(address(dai), 1, FerrataVault.GlobalDepositCapExceeded.selector);
        assertEq(_available(address(dai), DEPOSIT), 5000e18);
        assertEq(_globalAvailable(DEPOSIT), 500e6);
        assertEq(vault.escrowed(address(dai)), 0);
    }

    // ---- release caps ---------------------------------------------------------------------------------------------

    function test_tokenReleaseCap_exhaustsAndRefillsOverTime() public {
        for (uint256 i = 0; i < 3; i++) {
            _deposit(alice, ETH, 1, C);
        }
        _acceptRoot(ROOT);
        _setTokenCap(ETH, RELEASE, 2 ether, 0.001 ether); // refills 1 ETH per 1000 s

        _release(_newRelease(ETH, 1));
        _release(_newRelease(ETH, 1));
        FerrataVault.ReleaseRequest memory third = _newRelease(ETH, 1);
        vm.expectRevert(FerrataVault.TokenReleaseCapExceeded.selector);
        vault.release(third, SEAL);
        assertFalse(vault.nullifierUsed(third.nullifier));
        assertEq(vault.escrowed(ETH), 1 ether);

        vm.warp(block.timestamp + 999);
        vm.expectRevert(FerrataVault.TokenReleaseCapExceeded.selector);
        vault.release(third, SEAL);
        vm.warp(block.timestamp + 1);
        vault.release(third, SEAL);
        assertEq(recipient.balance, 3 ether);
    }

    function test_globalReleaseCap_countsValueAcrossTokens() public {
        _deposit(alice, address(usdc), 1, C);
        _deposit(alice, address(usdc), 1, C);
        _deposit(alice, ETH, 1, C);
        _acceptRoot(ROOT);
        _setTokenCap(ETH, RELEASE, 10 ether, 0);
        _setGlobalCap(RELEASE, 2000e6, 0); // 2000 USD

        _release(_newRelease(address(usdc), 1)); // 1000 USD
        FerrataVault.ReleaseRequest memory eth = _newRelease(ETH, 1); // 3000 USD
        vm.expectRevert(FerrataVault.GlobalReleaseCapExceeded.selector);
        vault.release(eth, SEAL);
        assertEq(_available(ETH, RELEASE), 10 ether); // the ETH bucket was not charged
        assertFalse(vault.nullifierUsed(eth.nullifier));

        _release(_newRelease(address(usdc), 1)); // the second 1000 USD fits
        assertEq(_globalAvailable(RELEASE), 0);
    }

    // ---- disabling, updating, overflow ----------------------------------------------------------------------------

    function test_unlimitedCapacityDisablesTheCap() public {
        _setTokenCap(address(usdc), DEPOSIT, 0, 0); // closed
        _expectDepositRevert(address(usdc), 0, FerrataVault.TokenDepositCapExceeded.selector);
        _setTokenCap(address(usdc), DEPOSIT, UNLIMITED, 0);
        for (uint256 i = 0; i < 5; i++) {
            _deposit(alice, address(usdc), 2, C);
        }
        (uint256 capacity,, uint256 available) = vault.tokenCap(address(usdc), DEPOSIT);
        assertEq(capacity, UNLIMITED);
        assertEq(available, UNLIMITED);

        _setGlobalCap(DEPOSIT, 0, 0);
        _expectDepositRevert(address(usdc), 0, FerrataVault.GlobalDepositCapExceeded.selector);
        _setGlobalCap(DEPOSIT, UNLIMITED, 0);
        _deposit(alice, address(usdc), 2, C);
        assertEq(_globalAvailable(DEPOSIT), UNLIMITED);
    }

    function test_capUpdate_fromUnlimitedStartsFull() public {
        _setTokenCap(address(dai), DEPOSIT, 1000e18, 0);
        assertEq(_available(address(dai), DEPOSIT), 1000e18);
    }

    function test_capUpdate_loweringTakesEffectAtOnce() public {
        _setTokenCap(address(dai), DEPOSIT, 1000e18, 0);
        _setTokenCap(address(dai), DEPOSIT, 100e18, 0);
        assertEq(_available(address(dai), DEPOSIT), 100e18);
        _deposit(alice, address(dai), 0, C);
        _expectDepositRevert(address(dai), 0, FerrataVault.TokenDepositCapExceeded.selector);
    }

    function test_capUpdate_raisingAddsNoInstantHeadroom() public {
        _setTokenCap(address(dai), DEPOSIT, 200e18, 0);
        _deposit(alice, address(dai), 0, C);
        _deposit(alice, address(dai), 0, C);
        _setTokenCap(address(dai), DEPOSIT, 1000e18, 10e18);
        assertEq(_available(address(dai), DEPOSIT), 0);
        vm.warp(block.timestamp + 10);
        assertEq(_available(address(dai), DEPOSIT), 100e18);
    }

    function test_capUpdate_settlesRefillEarnedUnderOldRate() public {
        _setTokenCap(address(dai), DEPOSIT, 200e18, 1e18);
        _deposit(alice, address(dai), 0, C);
        _deposit(alice, address(dai), 0, C);
        vm.warp(block.timestamp + 50); // 50 DAI earned at the old rate
        _setTokenCap(address(dai), DEPOSIT, 200e18, 0);
        assertEq(_available(address(dai), DEPOSIT), 50e18);
        vm.warp(block.timestamp + 1000);
        assertEq(_available(address(dai), DEPOSIT), 50e18);
    }

    function test_refillSaturatesWithoutOverflow() public {
        _setTokenCap(address(dai), DEPOSIT, UNLIMITED - 1, UNLIMITED - 1);
        _deposit(alice, address(dai), 2, C);
        vm.warp(block.timestamp + 100 * 365 days);
        assertEq(_available(address(dai), DEPOSIT), UNLIMITED - 1);
        _deposit(alice, address(dai), 2, C);

        _setTokenCap(address(usdc), DEPOSIT, 100e6, UNLIMITED - 1);
        _deposit(alice, address(usdc), 0, C);
        assertEq(_available(address(usdc), DEPOSIT), 0);
        vm.warp(block.timestamp + 1);
        assertEq(_available(address(usdc), DEPOSIT), 100e6);
    }

    /// @notice Over any run, a deposit is accepted exactly when it fits in the bucket, and the total accepted never
    ///         exceeds capacity + refill * elapsed.
    function testFuzz_tokenDepositCap_boundsThroughput(uint256 seed) public {
        uint256 capacity = 1000e6;
        uint256 refill = 2e6; // 1000 USDC burst, 2 USDC per second
        _setTokenCap(address(usdc), DEPOSIT, capacity, refill);
        uint256 start = block.timestamp;
        uint256 total;
        for (uint256 i = 0; i < 30; i++) {
            seed = uint256(keccak256(abi.encode(seed, i)));
            vm.warp(block.timestamp + seed % 300);
            uint8 denomIndex = uint8((seed >> 128) % 3);
            uint256 amount = _amount(address(usdc), denomIndex);
            uint256 availableBefore = _available(address(usdc), DEPOSIT);

            bool ok = _tryDeposit(address(usdc), denomIndex, FerrataVault.TokenDepositCapExceeded.selector);
            assertEq(ok, amount <= availableBefore);
            if (ok) total += amount;
            assertLe(total, capacity + refill * (block.timestamp - start));
        }
        assertEq(vault.escrowed(address(usdc)), total);
    }
}
